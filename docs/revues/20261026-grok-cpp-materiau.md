Je relis le correctif C++ et la fabrication, sans toucher aux fichiers. Je vérifie d'abord le code autour de l'appel et ce que le SDK Windows expose réellement.## Défauts

### 1. `cpp/fenetre.h` — la relecture rend vrai alors que rien n'est peint

```cpp
if (FAILED(DwmSetWindowAttribute(poignee, 38, &voulu, sizeof(voulu))))
  return false;
int relu = 0;
if (FAILED(DwmGetWindowAttribute(poignee, 38, &relu, sizeof(relu))))
  return false;
return relu == voulu;
```

Sur Windows 10 et sur Windows 11 avant le build 22621, l'attribut 38 est refusé : `DwmSetWindowAttribute` renvoie un `HRESULT` d'échec (`E_INVALIDARG`), `FAILED` suffit, et la fonction rend faux. Ce cas-là est propre.

À partir de 22621, `DwmGetWindowAttribute` relit la valeur stockée sur la `HWND`. Le compositeur peut ne rien dessiner et la comparaison `relu == voulu` reste vraie. Trois cas mesurés dans la doc de Microsoft (`DWMWINDOWATTRIBUTE`, page à jour au 2026-03-09) :

- Les effets de transparence sont coupés, le contraste élevé est actif, ou la session est distante : le type de fond reste enregistré, DWM peint une couleur plate.
- Build 26100 et suivants — le poste d'essai cité dans le dossier est en 26300. `DWMWA_REDIRECTIONBITMAP_ALPHA` vaut faux par défaut : « the alpha is ignored and the redirection bitmap is treated as fully opaque ». Ce bitmap couvre le matériau. Qt 6.8.3 ne pose pas `WS_EX_NOREDIRECTIONBITMAP` (seulement si `QT_QPA_DISABLE_REDIRECTION_SURFACE` est défini, `qwindowswindow.cpp`).
- Le client peint par-dessus. Les volets sont opaques. La fonction a déjà rendu vrai.

`main.cpp` n'appelle `setColor(Qt::transparent)` qu'après ce vrai. `Qt::transparent` est `(0, 0, 0, 0)`. Là où l'alpha présenté est ignoré, ces pixels sortent noirs. Le bandeau Pomme est à l'opacité 0,8 : il se mélange à ce noir. Le témoin « relecture égale à la valeur posée » valide cet état.

Correction : garder Set et Get comme témoin « l'OS a accepté l'attribut », et rendre faux avant `setColor` si le dessin est impossible — `SPI_GETDISABLEOVERLAPPEDCONTENT`, `SPI_GETHIGHCONTRAST`, clé `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\EnableTransparency` à 0. Sur un build ≥ 26100, poser `DWMWA_REDIRECTIONBITMAP_ALPHA` (`BOOL` vrai) et ignorer l'échec sur un OS plus ancien. Les pixels doivent être prémultipliés : la swapchain D3D de Qt 6.8 l'est (`DXGI_ALPHA_MODE_PREMULTIPLIED` dans `qrhid3d11.cpp`) seulement quand `requestedFormat().alphaBufferSize() > 0`. La preuve visuelle reste une capture : Get ne la donne pas.

### 2. `cpp/main.cpp` — `setColor(Qt::transparent)` contre le binding `color` de la fenêtre

```cpp
if (!voulu.isEmpty() && fenetre::materiau(principale, voulu))
  principale->setColor(Qt::transparent);
```

Le style Fusion, celui posé dans `main.cpp`, lie la couleur dans son `ApplicationWindow.qml` (Qt 6.8.3) :

```qml
color: window.palette.window
```

`QQuickWindow::setColor` écrit la propriété C++ et ne retire pas ce binding. Il tient jusqu'à la prochaine évaluation de `palette.window`. Le QML prévoit ce changement sans redémarrage (les `Binding` sur `palette.window`, `Main.qml`). À ce moment le style réécrit une couleur opaque (`#f5f5f7` sous Pomme) et recouvre le matériau. L'attribut DWM, lui, reste. Personne ne rappelle `materiau()`.

`setColor` appelle aussi `setFormat` quand l'alpha change (`qquickwindow.cpp`, 6.8.3). `QWindow::setFormat` ne fait que mémoriser `requestedFormat`. La doc de Qt : après `create()`, la surface native n'est pas rejouée. `visible: true` fait créer la `HWND` pendant `engine.load()`, donc avant cet appel. Le scene graph lit `requestedFormat()` à la création de la swapchain, au premier cadre, en principe après `setColor` : le chemin D3D peut avoir l'alpha. Le format natif (`format().hasAlpha()`) reste sans alpha, donc Qt n'appelle pas `DwmEnableBlurBehindWindow`. Le rendu logiciel (`choisirRendu`, machines sans carte) n'a pas la swapchain DirectComposition : le clear transparent y est du noir.

Là où aucun volet ne peint : la couleur de clear. Avec une swapchain à alpha et un matériau réellement composé, Mica (2) ou l'acrylique de bureau (3). Avec l'alpha ignoré, du noir. Les volets peignent `palette.base` opaque. La barre de titre reste le chrome DWM.

Correction : retirer le binding par `QQmlProperty::write(principale, "color", QColor(Qt::transparent))`, et le reposer quand l'apparence change. Demander l'alpha avant `create()` si le rendu n'est pas D3D — `visible: false` le temps de `setFormat` (alpha 8), du matériau, puis `show()`. `QQuickWindow::setDefaultAlphaBuffer(true)` avant `load()` évite le trou, et il fait aussi appeler `applyBlurBehindWindow` (ancien flou, région vide) parce que `format().hasAlpha()` est alors vrai : cet appel se mélange à `DWMWA_SYSTEMBACKDROP_TYPE`.

### 3. `cpp/fenetre.h` — Qt réécrit le cadre sombre

```cpp
BOOL sombre = TRUE;
DwmSetWindowAttribute(poignee, 20, &sombre, sizeof(sombre));
```

Qt 6.8.3 écrit le même attribut dans `QWindowsWindow::setDarkBorder`, sur `QEvent::ApplicationPaletteChange`, selon le thème Windows (`colorScheme() == Dark`), pas selon le bandeau. Thème clair : la valeur repasse à faux. L'échec de cet appel n'est pas testé ; sur Windows 10 le cadre sombre reste même quand le matériau a été refusé.

Correction : reposer `DWMWA_USE_IMMERSIVE_DARK_MODE` après un changement de palette, ou accepter le cadre que Qt calcule.

### 4. `cpp/fenetre.h` — constantes déjà nommées par le SDK

```cpp
DwmSetWindowAttribute(poignee, 20, &sombre, sizeof(sombre));
DwmSetWindowAttribute(poignee, 38, &voulu, sizeof(voulu));
```

Le SDK de la chaîne les définit. `socle.yml` construit avec MSVC sur `windows-2022`, sans `CMAKE_SYSTEM_VERSION` : le SDK par défaut de l'image est 10.0.22621 ou 10.0.26100. Dans `dwmapi.h` (présent depuis 10.0.22621, enum non gardé par `NTDDI_VERSION`) :

- `DWMWA_USE_IMMERSIVE_DARK_MODE` = 20
- `DWMWA_WINDOW_CORNER_PREFERENCE` = 33, puis bordure, légende, texte, épaisseur, donc `DWMWA_SYSTEMBACKDROP_TYPE` = 38
- `DWMSBT_NONE` = 1, `DWMSBT_MAINWINDOW` = 2 (Mica), `DWMSBT_TRANSIENTWINDOW` = 3 (acrylique de bureau), `DWMSBT_TABBEDWINDOW` = 4

Les nombres sont les bons. Le SDK 10.0.26100 ajoute `DWMWA_REDIRECTIONBITMAP_ALPHA` = 39. Un SDK arrêté à 22621 n'a pas ce nom-là : un `#ifndef` ne se justifie que pour 39.

Correction : inclure `dwmapi.h` (déjà fait) et utiliser les noms.

## Contrôles sans défaut

`winId()` ne crée pas une fenêtre cachée. `Main.qml` a `visible: true` : `engine.load()` a déjà fait `ShowWindow`. L'attribut 38 est posé sur une `HWND` affichée. Qt 6.8.3 ne réécrit pas cet attribut. `suivre()` (`MoveWindow`, `showMaximized`) ne recrée pas la `HWND`. Le déplacer avant le premier `show` reproduirait le faux témoin du défaut 1 : Get réussit, le compositeur n'applique qu'au show suivant, parfois seulement après un redimensionnement.

`SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &actives, 0)` est la signature de la doc. `SPI_GETCLIENTAREAANIMATION` vaut `0x1042`. `uiParam` est 0. `pvParam` attend un `BOOL*` : `actives` est un `BOOL` (int 32 bits), l'argument est son adresse. Le quatrième argument à 0 est celui d'une lecture (`SPIF_UPDATEINIFILE` et `SPIF_SENDCHANGE` sont pour l'écriture). En échec l'API rend `FALSE` et le code rend `true` : les animations restent actives, comme hors Windows. L'appel est une lecture locale, il ne bloque pas le démarrage.

`target_link_libraries(... dwmapi)` suffit à MSVC : CMake le résout en `dwmapi.lib`, comme `dxgi` et `ole32` sur la même ligne. `dwmapi.dll` est là depuis Vista. Sous Android et macOS, `Q_OS_WIN` est faux : `dwmapi.h` n'est pas inclus, `materiau()` et `animationsClientes()` compilent la branche qui rend faux ou vrai. `if(WIN32)` ne lie `dwmapi` que pour une cible Windows. L'appel depuis `main.cpp` sous macOS tombe dans cette branche et ne peint rien.

Aucune allocation. La `HWND` appartient à Qt. Pas d'usage après libération.

## Verdict : DÉFAUTS BLOQUANTS
