# Relecture Fable — accessibilité et politique de mouvement, 26/10/2026

Lentille : mouvement réduit, contrastes, survol, clavier et lecteurs d'écran.
Périmètre explicitement exclu : justesse du C++, non-régression des autres
apparences (confiés à Grok et Codex le même jour).

## Défauts retenus et corrigés

1. **Le survol de ligne était invisible.** Rectangle posé à `z: -1`, sous le
   fond du délégué, opaque dans le style Fusion. Le dépôt documentait déjà le
   piège, dans le commentaire de la rubrique Favoris de
   `LigneArborescence.qml`. Corrigé à `z: -0.25`.
2. **L'objet non lu tombait sous AA sur une ligne survolée**, 4,27:1 :
   `#0071e3` sur `#edf5fd`. Il reprend l'encre du texte sous la teinte de
   survol ; son gras et son bandeau de non-lus portent encore l'état.
3. **L'encre sur l'accent était sous AA** : `#161617` sur `#0071e3` à 3,85:1,
   et 3,06:1 à l'appui du bouton principal. Clé facultative `encreAccent`,
   blanche sous Pomme, absente ailleurs donc sans effet sur MMEDIA.
4. **Le `scale` d'appui rétrécissait la zone cliquable** (Qt teste le contact
   dans le repère transformé). Porté sur le fond, pas sur le contrôle.
5. **Une animation préexistante échappait au garde** : `Behavior on x` du
   curseur d'interrupteur. Passée sous `anime` : une animation qui y échappe
   rend le garde mensonger.

## Contrastes mesurés, apparence Pomme

Tous ≥ 4,5:1 sauf mention. Texte `#1d1d1f` : 15,46:1 sur `#f5f5f7`, 16,83:1
sur blanc. `texteAccent #0071e3` : 4,70:1 sur blanc, **4,31:1 sur `#f5f5f7`**
(barre d'état, lien survolé — sous le seuil, défaut préexistant de la
palette, à surveiller). `rubrique #6e6e73` : 4,66:1 sur `#f5f5f7`, 5,07:1 sur
blanc. `alerte #d70015` : 5,38:1 sur blanc. `texteBandeau #f5f5f7` sur
bandeau : 16,61:1 opaque, et **jamais sous 8,8:1 même composé à 0,8 sur un
bureau blanc** — la translucidité ne met pas le contraste en danger.

## Dette SIGNALÉE, non corrigée

Elle précède ce lot et touche les quatre apparences existantes ; la corriger
dépasserait un patch d'apparence.

- **Aucun indicateur de focus clavier** sur les boutons de l'en-tête ni sur
  les boutons d'action : zéro occurrence de `visualFocus` dans `core/qml`,
  alors que `AbstractButton` a `Qt::StrongFocus` et que Tab les atteint
  (WCAG 2.4.7). Correction proposée : `border.width: ctl.visualFocus ? 2 : …`
  et `border.color` pris dans l'accent ou l'encre du bandeau, couleurs déjà
  dans la table.
- **Drapeau et corbeille d'une ligne n'apparaissent qu'au survol**
  (`LigneMessage.qml`), donc hors de portée d'un utilisateur au clavier ; les
  infobulles ne viennent pas au focus non plus.
- **`AnimatedImage`** des messages : lecture liée à la visibilité, pas au
  réglage d'animation du poste (WCAG 2.2.2 au-delà de 5 s).
- **`animationsClientes()` est lu une fois au démarrage** : un changement du
  réglage en cours de session n'est suivi qu'au relancement, faute d'écoute
  de `WM_SETTINGCHANGE`.
