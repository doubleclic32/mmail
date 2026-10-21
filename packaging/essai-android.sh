#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Installe l'APK sur l'émulateur courant, le lance, et vérifie qu'il tient debout.
#
# Ce contrôle vit dans un fichier, et non dans le workflow : l'action
# android-emulator-runner exécute son « script » **ligne par ligne, chacune dans
# un shell distinct** — une variable posée à la première ligne est perdue à la
# suivante, ce qui fait échouer en silence toute séquence écrite naïvement.
set -euo pipefail

PAQUET=fr.mmedia.mmail
ATTENTE=20

apk=$(find _build/android-build -name '*.apk' | head -n 1)
if [ -z "$apk" ]; then
  echo "aucun APK construit sous _build/android-build" >&2
  exit 1
fi
echo "APK : $apk"

# La fabrication rend un APK non signé, et Android refuse de l'installer
# (INSTALL_PARSE_FAILED_NO_CERTIFICATES). On le signe ici avec une clé créée pour
# ce seul essai : elle vit le temps du travail, ne sert qu'à l'émulateur, et n'a
# rien à voir avec la clé de test conservée pour les APK publiés.
if ! unzip -l "$apk" | grep -qE 'META-INF/.*\.(RSA|EC|DSA)'; then
  echo "APK non signé : signature avec une clé d'essai jetable"
  cle=$(mktemp -u /tmp/essai-XXXXXX.keystore)
  motdepasse=$(head -c 18 /dev/urandom | base64 | tr -d '/+=')
  keytool -genkeypair -keystore "$cle" -storepass "$motdepasse" -keypass "$motdepasse" \
    -alias essai -keyalg RSA -keysize 2048 -validity 30 \
    -dname "CN=MMail essai, O=M-Media, C=FR" > /dev/null
  outils=$(ls -d "$ANDROID_SDK_ROOT"/build-tools/* | sort -V | tail -n 1)
  aligne=$(mktemp -u /tmp/mmail-XXXXXX.apk)
  "$outils/zipalign" -f -p 4 "$apk" "$aligne"
  "$outils/apksigner" sign --ks "$cle" --ks-pass "pass:$motdepasse" \
    --key-pass "pass:$motdepasse" --ks-key-alias essai "$aligne"
  apk="$aligne"
fi

# L'émulateur de GitHub, lent, se dit démarré avant que ses services le soient :
# l'installation échouait parfois sur « Can't find service: package », et le
# lancement par monkey s'arrêtait sur l'ANR d'une application du système
# (com.android.phone), sans que MMail y soit pour rien. On attend le
# gestionnaire de paquets, on réessaie l'installation, on lance par « am start »
# et on relance une fois si MMail n'a pas démarré — mais un plantage de MMail
# (FATAL EXCEPTION à son nom) échoue tout de suite.
for i in $(seq 1 30); do
  adb shell pm path android > /dev/null 2>&1 && break
  sleep 5
done
installe=0
for essai in 1 2 3; do
  if adb install -r "$apk"; then
    installe=1
    break
  fi
  echo "installation refusée (essai $essai), nouvel essai dans 20 s" >&2
  sleep 20
done
[ "$installe" = 1 ] || { echo "installation impossible" >&2; exit 1; }

composant=$(adb shell cmd package resolve-activity --brief -c android.intent.category.LAUNCHER "$PAQUET" | tr -d '\r' | tail -n 1)
echo "Activité : $composant"
adb logcat -c
pid=""
for essai in 1 2; do
  adb shell am broadcast -a android.intent.action.CLOSE_SYSTEM_DIALOGS > /dev/null 2>&1 || true
  adb shell am start -W -n "$composant" || true
  sleep "$ATTENTE"
  adb logcat -d -t 800 > logcat.txt
  if grep -qE "FATAL EXCEPTION|Process: $PAQUET" logcat.txt && grep -q "$PAQUET" logcat.txt \
     && grep -A3 "FATAL EXCEPTION" logcat.txt | grep -q "$PAQUET"; then
    echo "MMail a planté au lancement :" >&2
    grep -iE 'mmail|AndroidRuntime|FATAL' logcat.txt | tail -40 >&2
    exit 1
  fi
  pid=$(adb shell pidof "$PAQUET" || true)
  [ -n "$pid" ] && break
  echo "MMail absent $ATTENTE s après le lancement (essai $essai)" >&2
done

if [ -z "$pid" ]; then
  echo "MMail ne tourne plus $ATTENTE s apres son lancement :" >&2
  grep -iE 'mmail|AndroidRuntime|FATAL|ANR in' logcat.txt | tail -40 >&2
  exit 1
fi

echo "MMail tourne (pid $pid)"

# Veille puis réveil (0.6.10). Le 21/10, Manu retrouve MMail sur un écran blanc
# après une mise en veille du téléphone, sans autre issue que de le fermer.
# MMail doit d'abord être affiché : tant que son interface se charge, la fenêtre
# est d'une seule couleur, et Android peut le dire « ne répond plus » sans que
# la veille y soit pour rien. Viennent alors une veille courte, puis une veille
# profonde (Doze forcé, mémoire réclamée à l'application, comme après une nuit).
# Au réveil, MMail disparu, une application qui ne répond plus (ANR), ou une
# fenêtre restée d'une seule couleur échouent. Captures et journal d'Android
# partent avec les artefacts du travail.
uni() {
  # Part, en %, de la couleur dominante au centre d'une capture brute (sans la
  # barre d'état ni celle de navigation). screencap sans -p : largeur, hauteur,
  # format, parfois l'espace colorimétrique, puis RGBA. Une fenêtre vide, ou
  # l'écran éteint, donne près de 100, quelle que soit la couleur.
  python3 -I -c '
import collections, struct, sys
d = open(sys.argv[1], "rb").read()
l, h = struct.unpack("<II", d[:8])
px = d[len(d) - l * h * 4:]
teintes = collections.Counter()
for y in range(h // 10, h * 9 // 10, 7):
    for x in range(0, l, 7):
        i = 4 * (y * l + x)
        teintes[(px[i] >> 3, px[i + 1] >> 3, px[i + 2] >> 3)] += 1
print(round(100 * teintes.most_common(1)[0][1] / max(sum(teintes.values()), 1)))
' "$1"
}
# attendre <nom> <secondes> : capture jusqu'à ce que la fenêtre ne soit plus
# d'une seule couleur, ou que le délai soit passé ; la dernière est gardée.
attendre() {
  local fin=$((SECONDS + $2)) u=100
  while :; do
    adb exec-out screencap > "$1.raw"
    u=$(uni "$1.raw")
    [ "$u" -lt 98 ] || [ "$SECONDS" -ge "$fin" ] && break
    sleep 3
  done
  adb exec-out screencap -p > "$1.png"
  echo "$u"
}
reveiller() {
  adb shell input keyevent KEYCODE_WAKEUP
  sleep 2
  adb shell wm dismiss-keyguard || true
}
echec=0
controler() {
  # controler <étape> <part de la couleur dominante> : MMail toujours là, sans
  # ANR, et sa fenêtre dessinée.
  local pid2
  pid2=$(adb shell pidof "$PAQUET" || true)
  adb logcat -d > "logcat-$1.txt"
  cat "logcat-$1.txt" >> logcat.txt
  adb shell dumpsys window | grep -E 'mCurrentFocus' || true
  if [ -z "$pid2" ]; then
    echo "$1 : MMail ne tourne plus" >&2
    echec=1
  elif [ "$pid2" != "$pid" ]; then
    echo "$1 : MMail a été relancé (pid $pid, puis $pid2)" >&2
  fi
  if grep -q "ANR in $PAQUET" "logcat-$1.txt"; then
    echo "$1 : MMail ne répond plus (ANR)" >&2
    echec=1
  fi
  if [ "$2" -ge 98 ]; then
    echo "$1 : fenêtre d'une seule couleur (${2} %)" >&2
    echec=1
  fi
}

# L'écran ne s'éteint pas de lui-même pendant l'attente : une capture d'écran
# éteint passait pour MMail affiché.
adb shell settings put system screen_off_timeout 1800000 || true
reveiller
avant=$(attendre veille-0-avant 300)
echo "Avant la veille : couleur dominante ${avant} %"
if [ "$avant" -ge 98 ]; then
  # L'émulateur, et non MMail : sans accélération, le chargement a déjà
  # dépassé deux minutes et demie. Signalé, sans faire échouer le travail.
  echo "::warning::Essai de veille non fait : MMail pas affiché 5 min après son lancement"
  adb logcat -d >> logcat.txt
  exit 0
fi

adb logcat -c
adb shell input keyevent KEYCODE_SLEEP
sleep 30
reveiller
courte=$(attendre veille-1-courte 60)
echo "Veille courte (30 s) : couleur dominante ${courte} % au réveil"
controler veille-courte "$courte"

adb logcat -c
adb shell input keyevent KEYCODE_SLEEP
sleep 5
# Doze n'entre pas sur secteur : l'émulateur se dit débranché le temps de l'essai.
adb shell dumpsys battery unplug || true
adb shell dumpsys deviceidle force-idle || true
adb shell am send-trim-memory "$PAQUET" COMPLETE || true
sleep 60
adb shell dumpsys deviceidle unforce || true
adb shell dumpsys battery reset || true
reveiller
profonde=$(attendre veille-2-profonde 60)
echo "Veille profonde (Doze, mémoire réclamée) : couleur dominante ${profonde} % au réveil"
controler veille-profonde "$profonde"

[ "$echec" = 0 ] || exit 1
echo "Réveil après veille : MMail toujours affiché"

# Connexion Microsoft intégrée (0.6.11) : sa page n'est plus chargée qu'au
# moment d'une connexion. Pas de vue web avant ; elle s'ouvre depuis la fenêtre
# d'ajout de compte ; Retour l'abandonne et la décharge sans fermer MMail ; et
# l'essai de la touche Retour qui suit vérifie que MMail se ferme ensuite. Les
# boutons se trouvent par l'arbre d'accessibilité d'Android (s'il ne les montre
# pas, l'étape est signalée et sautée) ; cet arbre n'étant pas rafraîchi après
# l'ouverture de la page, c'est la hiérarchie des vues de l'activité qui dit si
# la vue web est là.
# trouver <début du texte> (ou =<texte exact>) : centre du premier élément
# affiché qui porte ce texte, vide s'il n'y en a pas.
trouver() {
  adb shell uiautomator dump /sdcard/ui.xml > /dev/null 2>&1 || return 0
  adb exec-out cat /sdcard/ui.xml > ui.xml 2>/dev/null || return 0
  python3 -I -X utf8 -c '
import html, re, sys
for n in re.findall(r"<node [^>]*>", open(sys.argv[1], encoding="utf-8").read()):
    a = dict((k, html.unescape(v)) for k, v in re.findall(r" ([\w-]+)=\"([^\"]*)\"", n))
    b = re.match(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", a.get("bounds", ""))
    t = a.get("text") or a.get("content-desc") or ""
    if b and (t == sys.argv[2][1:] if sys.argv[2][:1] == "=" else t.startswith(sys.argv[2])):
        x1, y1, x2, y2 = map(int, b.groups())
        print((x1 + x2) // 2, (y1 + y2) // 2)
        break
' ui.xml "$1"
}
# toucher <texte> : touche l'élément une fois sa place stable — le clavier qui
# se retire fait encore glisser la fenêtre d'ajout de compte.
toucher() {
  local xy avant="" i
  for i in 1 2 3 4 5; do
    xy=$(trouver "$1")
    [ -n "$xy" ] && [ "$xy" = "$avant" ] && break
    avant=$xy
    sleep 2
  done
  cp ui.xml "ui-$2.xml" 2>/dev/null || true
  echo "$1 : ${xy:-introuvable}"
  [ -n "$xy" ] || return 1
  # shellcheck disable=SC2086
  adb shell input tap $xy
}
# vue_web <étape> : nombre de vues web de l'activité (hiérarchie gardée).
vue_web() {
  adb shell dumpsys activity top > "vues-$1.txt" 2>/dev/null || true
  grep -cE 'webkit\.WebView|WebView\{' "vues-$1.txt" || true
}
adb shell input keyevent KEYCODE_BACK
sleep 5
if [ "$(vue_web 0-avant)" != 0 ]; then
  echo "Connexion Microsoft : vue web chargée avant toute connexion" >&2
  exit 1
fi
# Le tiroir des dossiers, ouvert au lancement, recouvre le bouton : refermé
# d'un glissement vers la gauche.
if [ -n "$(trouver "Toutes les BàL")" ]; then
  adb shell input swipe 250 330 10 330 300
  sleep 3
fi
if ! toucher "Compte Microsoft" 1-compte; then
  echo "::warning::Connexion Microsoft non essayée : bouton introuvable dans l'arbre d'accessibilité"
else
  # Adresse demandée, puis Entrée : la page de Microsoft s'ouvre.
  sleep 3
  adb shell input text "essai.mmail@outlook.com"
  sleep 1
  adb shell input keyevent KEYCODE_ENTER
  sleep 10
  adb exec-out screencap -p > connexion-1-ouverte.png
  if [ "$(vue_web 1-ouverte)" = 0 ]; then
    echo "Connexion Microsoft : la page intégrée ne s'est pas ouverte" >&2
    exit 1
  fi
  adb shell input keyevent KEYCODE_BACK
  sleep 5
  adb exec-out screencap -p > connexion-2-retour.png
  if [ "$(vue_web 2-retour)" != 0 ]; then
    echo "Connexion Microsoft : vue web toujours là après Retour" >&2
    exit 1
  elif [ -z "$(adb shell pidof "$PAQUET" || true)" ]; then
    echo "Connexion Microsoft : Retour a fermé MMail au lieu d'abandonner la connexion" >&2
    exit 1
  fi
  echo "Connexion Microsoft : page ouverte, puis abandonnée par Retour"
fi

# Touche Retour (0.6.11). Le 21/10, Manu retrouve MMail sur un écran blanc après
# un appui sur Retour, sans autre issue que de le fermer. Chaque appui ferme ce
# qui est ouvert (clavier, fenêtre d'ajout de compte, tiroir) ; sur la liste, la
# fenêtre se ferme et MMail doit quitter : main() rendu, processus fini, retour
# au lanceur. MMail resté au premier plan sur une fenêtre d'une seule couleur, ou
# qui ne quitte pas, échoue ; relancé ensuite, il doit de nouveau s'afficher.
adb logcat -c
premier_plan() {
  adb shell dumpsys window | grep -m 1 'mCurrentFocus' | tr -d '\r' || true
}
u=0
for n in 1 2 3 4 5 6; do
  adb shell input keyevent KEYCODE_BACK
  sleep 5
  adb exec-out screencap > "retour-$n.raw"
  adb exec-out screencap -p > "retour-$n.png"
  u=$(uni "retour-$n.raw")
  focus=$(premier_plan)
  echo "Retour $n : couleur dominante ${u} %, pid $(adb shell pidof "$PAQUET" || echo aucun), ${focus}"
  case "$focus" in *"$PAQUET"*) ;; *) break ;; esac
done
for i in $(seq 1 10); do
  [ -z "$(adb shell pidof "$PAQUET" || true)" ] && break
  sleep 2
done
pid2=$(adb shell pidof "$PAQUET" || true)
adb logcat -d > logcat-retour.txt
cat logcat-retour.txt >> logcat.txt
grep -E "main\(\) returned|ActivityTaskManager.*$PAQUET" logcat-retour.txt | tail -20 || true
case "$(premier_plan)" in
  *"$PAQUET"*)
    if [ "$u" -ge 98 ]; then
      echo "Retour : MMail au premier plan sur une fenêtre d'une seule couleur (${u} %)" >&2
    else
      echo "Retour : MMail toujours au premier plan après $n appuis" >&2
    fi
    echec=1 ;;
esac
if [ -n "$pid2" ]; then
  echo "Retour : le processus de MMail tourne encore 20 s après la fermeture (pid $pid2)" >&2
  grep -q "main() returned" logcat-retour.txt || echo "Retour : main() n'a pas rendu la main" >&2
  echec=1
fi

adb logcat -c
adb shell am start -W -n "$composant" || true
relance=$(attendre retour-relance 120)
echo "Relance après Retour : couleur dominante ${relance} %"
pid=$(adb shell pidof "$PAQUET" || true)
controler retour-relance "$relance"

[ "$echec" = 0 ] || exit 1
echo "Retour : MMail quitte, puis se relance affiché"
