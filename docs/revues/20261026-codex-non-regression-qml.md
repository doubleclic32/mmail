**Une régression MMEDIA est reproduite.** Vérifications sur extraits isolés sous Qt 6.8.3/Fusion ; application complète et `--smoke` non exécutés.

1. **Bloquant — `core/qml/Main.qml`, `survolEntete()` : le survol sombre MMEDIA change de couleur.**

   Extrait :
   ```qml
   if (sombre)
       return Qt.rgba(1, 1, 1, valeurOu("survolBandeau", 0.14))
   ```

   La [table MMEDIA publiée](https://github.com/mmedia-fr/mmail/blob/c4f9e741469810e297ecf03740e3215e7e4db301/core/qml/Main.qml#L454) contient `texteBandeau: "#ffffff"`, une **chaîne JS**. L’ancienne `encreEntete(true)` retourne cette chaîne sans conversion en `color` : ses champs `.r/.g/.b` sont donc `undefined`. Sous Qt 6.8.3, l’ancienne formule produit du **noir à 14 %**, tandis que la nouvelle produit du **blanc à 14 %**.

   Vérification des quatre chemins, sans les nouvelles clés :

   | `marque` | `sombre` | Avant | Après |
   |---|---|---|---|
   | Nulle | Faux | `palette.windowText`, alpha 0,08 | Identique |
   | Nulle | Vrai | `palette.windowText`, alpha 0,08 | Identique |
   | MMEDIA | Faux | `palette.windowText`, alpha 0,08 | Identique |
   | MMEDIA | Vrai | Noir, alpha 0,14 | **Blanc, alpha 0,14** |

   Cela change les survols des onglets, boutons et menus utilisant cette fonction. L’assertion portant uniquement sur `.a` ne détecte rien.

   **Correction :** conserver l’ancien calcul lorsque la nouvelle clé est absente, et réserver le nouveau calcul à Pomme. Attention : **`BoutonEntete`, lui, reste actuellement identique**, puisque son ancien fond était explicitement blanc à 14 %. Pour préserver les deux comportements, lui laisser un calcul distinct :
   ```qml
   Qt.rgba(1, 1, 1, fenetre.valeurOu("survolBandeau", 0.14))
   ```

2. **`core/qml/BoutonAction.qml` et `core/qml/Main.qml`, boutons : `scale` peut annuler un clic valide.**

   Extrait :
   ```qml
   scale: ba.down && fenetre.anime ? 0.97 : 1
   ```
   Même défaut pour `be` et `bo`.

   La transformation réduit aussi la surface utilisée pour tester la présence du pointeur. Reproduction avec un bouton de 200 pixels : appui à 1 pixel du bord, attente de 150 ms, déplacement de 1 pixel vers l’intérieur, relâchement. Sans réduction : un clic. Avec réduction : **zéro clic, signal `canceled`**. Le pointeur reste pourtant dans le rectangle initial.

   Cela touche Pomme lorsque les animations sont autorisées.

   **Correction :** appliquer la réduction aux éléments visuels — fond et contenu — en conservant le contrôle interactif à `scale: 1`.

3. **`core/qml/LigneArborescence.qml` et `core/qml/LigneMessage.qml` : le nouveau survol est caché sous Fusion.**

   Extrait :
   ```qml
   Rectangle {
       z: -1
       anchors.fill: parent
       color: /* teinte de survol */
   }
   ```

   Le fond du contrôle est lui aussi à `z: -1`. Dans les deux reproductions, il est créé après ce rectangle et le recouvre avec son fond opaque. Résultat : `hovered === true`, couleur de survol correctement calculée, mais pixel affiché toujours blanc.

   **Correction :** placer explicitement le voile au-dessus des fonds et sous le contenu. Dans cette structure, `z: -0.25` fonctionne, y compris au-dessus du fond Favoris à `-0.5`. Vérifier également son placement pendant le glissement du `SwipeDelegate`.

4. **`core/qml/Main.qml`, animations d’onglet : désactiver `anime` ne termine pas une animation déjà lancée.**

   Extrait :
   ```qml
   Behavior on width {
       enabled: fenetre.anime
       NumberAnimation { duration: 220; easing.type: Easing.OutCubic }
   }
   ```

   `enabled` contrôle le déclenchement du comportement ; sa désactivation seule n’arrête pas l’animation courante. [Documentation Qt](https://doc.qt.io/qt-6/qml-qtquick-behavior.html#enabled-prop).

   Reproduction : désactivation pendant l’ouverture d’un filet de 200 pixels. Sa largeur passe encore d’environ **109 à 182**, puis à 200, avec `anime === false`. Un passage de Pomme à MMEDIA pendant cette animation conserve donc momentanément un comportement animé sous MMEDIA.

   **Correction :** afficher directement la valeur cible lorsque `anime` est faux, par exemple en séparant largeur cible et largeur animée :
   ```qml
   width: fenetre.anime ? largeurAnimee : largeurCible
   ```
   Appliquer le même principe aux autres propriétés animées.

5. **`core/qml/Main.qml`, scénario `--smoke` : certaines assertions ne protègent pas leurs consommateurs.**

   Extraits :
   ```qml
   valeurOu("filetBandeau", 3) !== 3
   survolEntete(true).a.toFixed(2) !== "0.14"
   ```

   Sans clé `filetBandeau`, le premier contrôle compare le défaut fourni par le test à ce même défaut. Mutation vérifiée : remplacer **dans le rectangle réel** le défaut `3` par `0` laisse ce contrôle vert, alors que le filet disparaît. Le deuxième accepte indifféremment le noir et le blanc du défaut nº 1.

   Ces assertions ne sont pas toutes impossibles à faire tomber : modifier la table peut faire échouer celle du filet ; celles d’`anime` et de `materiauVoulu` peuvent détecter une erreur de calcul. Elles ne prouvent toutefois pas le comportement des contrôles affichés.

   **Correction :** vérifier la hauteur du rectangle réel, les couleurs RGBA complètes, et le comportement après désactivation des animations. Ajouter les cas non marqués et faire échouer ces tests par mutation.

Les autres défauts numériques de `valeurOu` correspondent aux valeurs antérieures. Aucun piège d’ASI dans les `return` coupés, aucune boucle de liaison, propriété masquée ou cible de `Behavior` inexistante repérés dans les ajouts.

## Verdict : DÉFAUTS BLOQUANTS