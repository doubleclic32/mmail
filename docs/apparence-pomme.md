# Apparence « Pomme » : matériau et mouvement

Ce document a servi de plan, puis de compte rendu. **Tout ce qu'il décrit est
écrit et fabriqué** sur la branche `theme-pomme` ; ce qui n'a pas été fait est
dit tel quel, à la fin.

La branche est posée sur la **0.6.20** (`afc0224`). Le survol du bandeau MMEDIA
signalé ici est corrigé par l'auteur dans la 0.6.21 (`Qt.lighter(encreEntete
(sombre), 1.0)`) : le témoin de non-régression a été réécrit pour passer sur
les deux versions sans rien laisser filtrer (cf. chantier 1).

L'apparence `pomme` reprend l'habillage d'un site connu : fonds gris très
clairs, surfaces blanches, bleu système en sélection et en accent, bandeau
anthracite. Un thème seul n'y suffisait pas, d'où les trois chantiers
ci-dessous : un survol qu'on voit, du mouvement sur le bandeau, et la matière
translucide de Windows 11.

## État, au 26/10/2026

| Chantier | État | Commit |
|---|---|---|
| Apparence `pomme` | fait | `Apparence « Pomme » : la charte…` |
| Clés facultatives et survol visible | fait | `Apparence : clés facultatives…` |
| Garde de mouvement | fait | `Mouvement : le garde…` |
| Transitions | fait | `Mouvement : les transitions…` |
| Matériau Windows 11 | fait | `Matériau translucide…` |
| Correctifs de relecture | fait | deux commits, cf. `docs/revues/` |

Fabrication : les travaux `windows`, `linux` et les deux `android` sont au vert,
`--smoke` compris, donc le scénario intégré et ses assertions nouvelles. Le
travail `macos` a été annulé par GitHub, sans aucune étape enregistrée, sur
trois exécutions successives d'un dépôt dérivé ; rien dans le code ne le
provoque.

Mesuré sur un poste Windows 11 build 26300, par les pixels d'une capture :

```
bandeau (zone vide)   #161616  alpha=204     (204/255 = 0,8 : l'opacité demandée)
fond de la fenetre    #ffffff  alpha=255
alphas les plus frequents : 255 (956860 px), 204 (44076 px), 0 (11078 px)
```

Les pixels à alpha nul prouvent que la fenêtre porte une vraie transparence,
donc que la liaison du style sur `color` a bien été retirée et que le chemin du
matériau est engagé. **Ce qu'aucune mesure d'ici ne prouve** : qu'un compositeur
peigne effectivement Mica derrière. Une capture de fenêtre ne contient pas ce
qui est composé sous elle, et l'API DWM ne le dit pas non plus (cf. chantier 3).

Les anchors de ligne renvoient à `core/qml/Main.qml` au commit
`Apparence « Pomme » : « Inter Display » pour les titres`.

## Principe directeur : tout reste dans la table

Le mécanisme d'apparence est une table de données (`jeuxApparence`, ligne 424) que
quatre apparences remplissent. **Rien de ce qui suit n'est codé en dur dans les
composants** : chaque comportement nouveau est piloté par une clé **optionnelle**
de cette table. Une apparence qui ne porte pas la clé garde exactement le
comportement d'aujourd'hui, au pixel près. C'est la condition pour que
`classique`, `moderne`, `systeme` et `mmedia` ne bougent pas.

Clés nouvelles, toutes optionnelles :

| Clé | Défaut quand absente | Rôle |
|---|---|---|
| `survolBandeau` | `0.14` | opacité du survol sur le bandeau sombre |
| `survolBande` | encre neutre à `0.08` | survol sur la bande claire et hors bandeau |
| `filetBandeau` | `3` | hauteur du filet sous l'en-tête, en points |
| `mouvement` | `false` | autorise les transitions décrites au chantier 2 |
| `materiau` | `"aucun"` | `"mica"`, `"acrylique"` ou `"aucun"` (chantier 3) |
| `opaciteBandeau` | `1.0` | opacité du bandeau, pour laisser voir le matériau |

Valeurs retenues pour `pomme` : `survolBandeau: 0.18`, `survolBande` à l'accent
à `0.14`, `filetBandeau: 1`, `mouvement: true`, `materiau: "mica"`,
`opaciteBandeau: 0.8`. Ce `0.8` est celui du haut d'`apple.com`, où le chrome est
`rgba(22,22,23,0.8)`.

## Chantier 1 — un survol qu'on voit

**Constat mesuré.** `survolEntete(sombre)` (ligne 565) rend l'encre de l'en-tête à
`0.14` d'opacité sur le bandeau sombre, et à `0.08` partout ailleurs. Dans la
disposition `etages`, les boutons et les interrupteurs descendent sur la bande
claire (`header:`, ligne 1176) et n'ont donc qu'un noir à 8 % au survol : à la
limite du perceptible sur un fond `#f5f5f7`. C'est le premier reproche formulé à
l'essai, et il est juste.

**Changement.** Une seule fonction à reprendre, ligne 565 :

```qml
function survolEntete(sombre) {
    // Hors apparence de marque : inchangé, l'encre du style à 8 %.
    if (marque === null) {
        var e = encreEntete(sombre)
        return Qt.rgba(e.r, e.g, e.b, 0.08)
    }
    if (sombre)
        return Qt.rgba(1, 1, 1, valeurOu("survolBandeau", 0.14))
    // Sur la bande claire, l'accent plutôt qu'un gris : à faible opacité, une
    // teinte colorée se voit là où un noir neutre se confond avec l'ombre.
    var a = accent
    return Qt.rgba(a.r, a.g, a.b, valeurOu("survolBande", 0.08))
}
```

`valeurOu(clef, defaut)` est une fonction d'une ligne à poser près de
`valeurJeu` (ligne 845) : elle rend la clé de l'apparence courante si elle
existe, le défaut sinon. C'est elle qui garantit l'immobilité des quatre autres
apparences, et il n'y a donc qu'un seul endroit où se tromper.

**Épreuve.** Dans la fonction de contrôle appelée par `--smoke`, affirmer les
deux côtés du garde. Le témoin de non-régression sur l'apparence de l'auteur
est le seul qui compte vraiment, et il est écrit pour tenir **avant comme
après** la correction annoncée pour la 0.6.21 :

```qml
choisirApparence("mmedia")
// Clair : l'encre est palette.windowText, une vraie couleur, l'attendu est exact.
var encreClaire = encreEntete(false)
var attenduClair = Qt.rgba(encreClaire.r, encreClaire.g, encreClaire.b, 0.08)
if (survolEntete(false).toString() !== attenduClair.toString())
    return "survol clair MMEDIA modifié : " + survolEntete(false)
// Sombre : l'encre est marque.texteBandeau, une CHAÎNE. Un attendu « noir à
// 14 % » tomberait sur la 0.6.21, un attendu « blanc à 14 % » tombe sur la
// 0.6.20. Ce qu'un patch d'apparence ne doit pas changer vaut dans les deux
// cas : une teinte NEUTRE, à 14 %.
var sv = survolEntete(true)
if (sv.a.toFixed(2) !== "0.14" || sv.r !== sv.g || sv.g !== sv.b)
    return "survol sombre MMEDIA modifié : " + sv
choisirApparence("pomme")
if (survolEntete(true).toString() !== Qt.rgba(1, 1, 1, 0.18).toString())
    return "survol de l'apparence Pomme non appliqué : " + survolEntete(true)
```

Ce garde a été joué sur quatre mutants avant d'être retenu — fuite des clés de
Pomme sur MMEDIA dans les deux versions, opacité portée à 0,20, teinte
remplacée par l'accent cyan — et il rougit sur les quatre, tout en restant
vert sur les deux expressions légitimes.

## Chantier 2 — le mouvement

**Un seul interrupteur, honoré partout.** Le mouvement ne dépend pas que de
l'apparence : le système peut le refuser. Sous Windows, c'est
`SystemParametersInfo(SPI_GETCLIENTAREAANIMATION)`, l'équivalent du
`prefers-reduced-motion` du web. À exposer depuis `cpp/main.cpp`, à côté de
`modeControle` (ligne 400), en propriété de contexte `animationsSysteme`, vraie
par défaut hors Windows. Puis, dans `Main.qml` :

```qml
// Le mouvement demande les deux : l'apparence l'autorise, le système ne
// l'interdit pas. Un « Behavior » dont « enabled » est faux ne retarde rien :
// la propriété saute à sa valeur, comme aujourd'hui.
readonly property bool anime: valeurOu("mouvement", false) === true && animationsSysteme
```

Chaque transition qui suit porte `enabled: fenetre.anime`. **Aucune animation ne
doit s'écrire sans ce garde**, sinon le réglage d'accessibilité du poste est
ignoré.

**Où, exactement, et avec quelles valeurs.**

1. **Le filet de l'onglet actif coulisse** au lieu d'apparaître. Aujourd'hui
   chaque `OngletEntete` (ligne 571) porte son propre rectangle de 3 points,
   `visible: og.checked`. Le remplacer par **un seul** filet, enfant du
   `RowLayout` qui tient les onglets, dont `x` et `width` suivent l'onglet
   coché, avec `Behavior on x` et `Behavior on width`,
   `NumberAnimation { duration: 220; easing.type: Easing.OutCubic }`. Quand
   `anime` est faux, le filet se repositionne sans transition : le rendu
   d'arrivée est identique, c'est le trajet qui disparaît.
2. **Le libellé d'un onglet se révèle.** `opacity: og.checked || og.hovered ? 1 : 0.72`
   reçoit `Behavior on opacity { NumberAnimation { duration: 140 } }`.
3. **Les pastilles de l'en-tête se remplissent en fondu.** Le rectangle de fond
   de `BoutonEntete` (ligne 512) et celui de `BoutonOutil` (ligne 606) reçoivent
   `Behavior on color { ColorAnimation { duration: 140 } }`, et le même sur
   `border.color`.
4. **L'appui se voit.** Sur ces deux composants et sur `BoutonAction.qml` :
   `scale: down ? 0.97 : 1`, avec
   `Behavior on scale { NumberAnimation { duration: 90; easing.type: Easing.OutQuad } }`.
   Un retour d'appui de moins de 100 ms se perçoit comme une réaction, au-delà
   comme un retard.
5. **La ligne de message réagit au passage.** `LigneMessage.qml` n'a aucun fond
   de survol aujourd'hui : seules les icônes de drapeau et de corbeille
   apparaissent (lignes 344 et 384). Ajouter derrière la ligne un rectangle
   `color: ligne.highlighted ? palette.highlight
        : ligne.hovered ? Qt.rgba(accent.r, accent.g, accent.b, 0.07) : "transparent"`,
   avec `Behavior on color { ColorAnimation { duration: 120 } }`. Le `0.07` est
   bas volontairement : sur une liste longue, un survol trop marqué fatigue.
6. **Le dossier de l'arborescence de même**, dans `LigneArborescence.qml`, qui
   n'utilise aujourd'hui que `highlighted` (ligne 27).

**Ce qu'il ne faut pas animer.** Rien qui se déclenche sur l'arrivée de données :
pas de pulsation du compteur de non-lus, pas d'apparition en cascade des lignes
quand un dossier se charge. Une liste de courrier se remplit à un rythme que
l'utilisateur ne commande pas ; animer ce rythme donne une interface qui
frémit toute seule.

**Épreuve.** Le contrôle de `--smoke` affirme les trois états du garde, le
troisième étant celui qu'on oublie :

```qml
// L'apparence autorise et le système autorise : ça anime.
// L'apparence refuse : ça n'anime pas, même si le système autorise.
// Le système refuse : ça n'anime pas, même sous Pomme.
```

Le dernier se joue en forçant `animationsSysteme` à faux. **Un garde doit être
vu en train de déclencher** : coller la sortie du cas qui l'a fait refuser.

## Chantier 3 — la matière de Windows 11

**Ce que c'est, et ce que ce n'est pas.** Windows 11 sait servir un matériau
translucide et flouté à une fenêtre : Mica, qui prend le fond du bureau, et
Acrylique, qui prend ce qui est derrière la fenêtre. C'est le compositeur du
système qui le dessine, pas l'application : il n'y a donc **aucun nuanceur à
écrire et aucun coût d'affichage côté MMail**. En revanche le matériau se pose
sur la fenêtre entière, pas sur une bande choisie. Comme les trois volets de
MMail peignent leur fond opaque (`palette.base`), le matériau n'apparaît que là
où rien ne peint : le bandeau de l'en-tête, si on le rend partiellement
transparent, et les bords de la fenêtre.

**Prérequis mesuré sur le poste d'essai** : `Microsoft Windows 11 Professionnel`,
build `26300`. `DWMWA_SYSTEMBACKDROP_TYPE` demande 22621 ou plus. À garder sous
un contrôle de version du système (`RtlGetVersion`), pas sous une simple
compilation conditionnelle : le binaire Windows tourne aussi sur des postes en
Windows 10, où l'appel doit être sans effet et sans erreur.

**Trois changements.**

1. `CMakeLists.txt`, ligne 198 : ajouter `dwmapi` à
   `target_link_libraries(mmail PRIVATE ole32 shell32 shlwapi uuid advapi32 dxgi)`.
2. `cpp/fenetre.h`, qui inclut déjà `<windows.h>` et travaille déjà sur
   `QQuickWindow` : une fonction `materiau(QQuickWindow*, Materiau)` qui pose
   `DWMWA_USE_IMMERSIVE_DARK_MODE` (20) puis `DWMWA_SYSTEMBACKDROP_TYPE` (38)
   avec 2 pour Mica, 3 pour Acrylique. Elle rend un booléen : **vrai seulement
   si `DwmGetWindowAttribute` relit la valeur qu'on vient de poser**. Un appel
   DWM qui échoue est silencieux ; sans relecture, on croirait avoir posé le
   matériau.
3. `cpp/main.cpp` : appeler `materiau()` après la création de la fenêtre, près
   de `fenetre::suivre` (ligne 538) mais **hors de sa condition** — celle-ci
   écarte `--smoke`, `--capture` et `MMAIL_TAILLE`, or une capture doit montrer
   le matériau. Et poser `fenetre->setColor(Qt::transparent)` quand le matériau
   est accordé, sinon le fond opaque de la fenêtre le recouvre.

Côté QML, le bandeau (ligne 920, le rectangle de fond de `menuBar`) prend
`color: Qt.rgba(bandeau.r, bandeau.g, bandeau.b, valeurOu("opaciteBandeau", 1.0))`.

**Épreuve.** La relecture DWM est le témoin positif. Le témoin négatif : sur un
poste en Windows 10, ou avec `materiau: "aucun"`, la fonction doit rendre faux
et la fenêtre rester opaque. Une capture par `--capture` avant et après prouve
le reste ; c'est le seul des trois chantiers dont la preuve est une image.

## Ce qu'on ne fait pas

**Le flou d'une barre sur le contenu qui défile dessous.** Techniquement
possible : les binaires officiels sont fabriqués avec Qt 6.8.3 (cf.
`QT_VERSION` dans `.github/workflows/socle.yml`), donc `MultiEffect` de
`QtQuick.Effects` est disponible. Mais `CMakeLists.txt` déclare Qt 6.4 comme
plancher, et `MultiEffect` n'existe qu'à partir de 6.5 : l'utiliser obligerait
soit à relever le plancher, soit à écrire deux chemins. Pour un gain faible à
côté du matériau du système, et un coût d'affichage réel sur une liste longue,
ça ne vaut pas la dette. À reconsidérer si le plancher monte un jour.

## Ce qui a changé après relecture

Trois relectures croisées, lentilles disjointes, rendus entiers dans
`docs/revues/`. Elles ont trouvé sept défauts, dont deux que l'auteur de ce
lot n'aurait pas vus seul :

- **Une régression sur MMEDIA.** `survolEntete()` réécrite rendait du blanc sur
  le bandeau sombre, là où l'expression d'origine rend autre chose :
  `encreEntete(true)` rend `marque.texteBandeau`, qui est une **chaîne JS**
  puisque `marque` est la table (un `var`), donc `.r/.g/.b` y sont indéfinis.
  L'expression d'origine est maintenant reproduite telle quelle, particularité
  comprise. **Ce n'est pas à un patch d'apparence de corriger ça**, mais c'est
  signalé ici : le survol du bandeau MMEDIA n'est probablement pas la couleur
  que son auteur croit poser.
- **Le survol de ligne était invisible**, posé à `z: -1`, sous le fond du
  délégué, opaque dans le style Fusion. Le dépôt documentait déjà le piège,
  dans le commentaire de la rubrique Favoris de `LigneArborescence.qml`.

Et deux gardes qui ne gardaient rien, prouvés par mutation : une assertion qui
comparait la valeur par défaut que le test se passait à lui-même, et une autre
qui ne regardait que l'opacité d'une couleur dont c'est la teinte qui avait
changé.

## Dette signalée, NON corrigée

Elle précède ce lot et touche les quatre apparences existantes. La corriger
dépasserait un patch d'apparence, et c'est au propriétaire du code d'en
décider.

- **Aucun indicateur de focus clavier** sur les boutons de l'en-tête ni sur les
  boutons d'action : zéro occurrence de `visualFocus` dans `core/qml`, alors
  que `AbstractButton` a `Qt::StrongFocus` et que Tab les atteint (WCAG 2.4.7).
- **Drapeau et corbeille d'une ligne n'apparaissent qu'au survol**, donc hors
  de portée au clavier ; les infobulles ne viennent pas au focus non plus.
- **`texteAccent` sur le fond de fenêtre** : `#0071e3` sur `#f5f5f7` fait
  4,31:1, sous le seuil AA, pour la barre d'état et le lien survolé.
- **Le matériau est lu une seule fois au démarrage** : changer d'apparence en
  cours de session ne le pose ni ne le retire avant le relancement.
- **`animationsClientes()` est lu une seule fois** : un changement du réglage
  d'animation du poste n'est suivi qu'au relancement, faute d'écoute de
  `WM_SETTINGCHANGE`.

## Ordre des commits

1. `valeurOu()` et les clés de la table, sans aucun usage : le socle, qui ne
   change rien à l'écran et se relit en deux minutes.
2. Chantier 1, avec son témoin de non-régression sur `mmedia`.
3. `animationsSysteme` et `anime`, sans aucune transition encore branchée.
4. Chantier 2, une transition par commit pour que chacune se juge seule.
5. Chantier 3, qui est le seul à toucher du C++ et le seul propre à Windows.

Chaque commit passe `--smoke` et la chaîne `socle.yml`. Les tests verts d'une
cible de compilation partagée ne valent pas preuve : la campagne qui compte
tourne dans une cible neuve, sur les octets finaux.
