// SPDX-License-Identifier: GPL-3.0-or-later
// Position de la fenêtre principale : retenue d'une session à l'autre, remise
// sur un écran visible si elle n'y tombe plus, et « Ramener la fenêtre » au
// clic droit sur la barre des tâches.
//
// Une position retenue peut tomber hors de tout écran : écran secondaire
// débranché, poste repris sur un autre bureau, résolution changée. Au
// démarrage, elle n'est reprise que si la barre de titre est sur un écran ;
// sinon la fenêtre s'ouvre au centre de l'écran principal. Et à tout moment,
// le clic droit sur l'icône de la barre des tâches propose « Ramener la
// fenêtre » — tâche de la liste de raccourcis sous Windows, action du lanceur
// sous Linux (cf. packaging/mmail.desktop) : elle lance `mmail --ramener`, qui
// le demande à l'instance ouverte par le canal d'instance unique.
#pragma once

#include <QtCore/QObject>
#include <QtCore/QRect>
#include <QtCore/QSettings>
#include <QtCore/QTimer>
#include <QtGui/QGuiApplication>
#include <QtGui/QScreen>
#include <QtQuick/QQuickWindow>

#include <memory>

#ifdef Q_OS_WIN
#include <windows.h>
#include <dwmapi.h>
#include <propidl.h>
#include <shobjidl.h>
#endif

namespace fenetre {

// Vrai si la barre de titre d'un cadre est au moins en partie sur un écran :
// de quoi saisir la fenêtre à la souris.
inline bool saisissable(const QRect& cadre) {
  const int haut = cadre.top() + 12;
  for (const QPoint point : {QPoint(cadre.center().x(), haut), QPoint(cadre.left() + 80, haut),
                             QPoint(cadre.right() - 80, haut)}) {
    if (QGuiApplication::screenAt(point))
      return true;
  }
  return false;
}

// Cadre centré sur l'écran principal, à la taille donnée, réduite s'il le faut
// pour tenir dans la partie utilisable de l'écran.
inline QRect auCentre(QSize taille) {
  const QScreen* ecran = QGuiApplication::primaryScreen();
  if (!ecran)
    return QRect(QPoint(40, 40), taille);
  const QRect zone = ecran->availableGeometry();
  taille = taille.boundedTo(zone.size() - QSize(40, 40));
  QRect cadre(QPoint(0, 0), taille);
  cadre.moveCenter(zone.center());
  return cadre;
}

// Remet la fenêtre au centre de l'écran principal, à l'état normal, au premier
// plan.
inline void ramener(QQuickWindow* f) {
  f->setWindowStates(Qt::WindowNoState);
  f->setGeometry(auCentre(f->size()));
  f->show();
  f->raise();
  f->requestActivate();
}

// Reprend la position retenue, si elle est saisissable ; puis la retient à
// chaque changement. Une fenêtre agrandie retient sa position normale et
// l'état agrandi.
inline void suivre(QQuickWindow* f) {
  QSettings reglages;
  reglages.beginGroup(QStringLiteral("fenetre"));
  const QRect retenue(reglages.value(QStringLiteral("x"), 0).toInt(), reglages.value(QStringLiteral("y"), 0).toInt(),
                      reglages.value(QStringLiteral("largeur"), 0).toInt(),
                      reglages.value(QStringLiteral("hauteur"), 0).toInt());
  const bool agrandie = reglages.value(QStringLiteral("agrandie"), false).toBool();
  reglages.endGroup();
  if (retenue.width() >= 200 && retenue.height() >= 150) {
    if (saisissable(retenue))
      f->setGeometry(retenue);
    else
      f->setGeometry(auCentre(retenue.size()));
  }
  if (agrandie)
    f->showMaximized();

  // Position normale (hors agrandissement), écrite une seconde après le
  // dernier changement, et à la fermeture si elle ne l'est pas encore.
  auto normale = std::make_shared<QRect>(f->geometry());
  auto* ecriture = new QTimer(f);
  ecriture->setSingleShot(true);
  ecriture->setInterval(1000);
  const auto ecrire = [f, normale] {
    QSettings reglages;
    reglages.beginGroup(QStringLiteral("fenetre"));
    reglages.setValue(QStringLiteral("x"), normale->x());
    reglages.setValue(QStringLiteral("y"), normale->y());
    reglages.setValue(QStringLiteral("largeur"), normale->width());
    reglages.setValue(QStringLiteral("hauteur"), normale->height());
    reglages.setValue(QStringLiteral("agrandie"), bool(f->windowStates() & Qt::WindowMaximized));
    reglages.endGroup();
  };
  QObject::connect(ecriture, &QTimer::timeout, f, ecrire);
  QObject::connect(qApp, &QCoreApplication::aboutToQuit, ecriture, [ecriture, ecrire] {
    if (ecriture->isActive()) {
      ecriture->stop();
      ecrire();
    }
  });
  const auto change = [f, normale, ecriture] {
    if (f->windowStates() == Qt::WindowNoState && f->isVisible())
      *normale = f->geometry();
    ecriture->start();
  };
  QObject::connect(f, &QWindow::xChanged, f, change);
  QObject::connect(f, &QWindow::yChanged, f, change);
  QObject::connect(f, &QWindow::widthChanged, f, change);
  QObject::connect(f, &QWindow::heightChanged, f, change);
  QObject::connect(f, &QWindow::windowStateChanged, f, change);
}

#ifdef Q_OS_WIN
// Tâche « Ramener la fenêtre » de la liste de raccourcis : le menu du clic
// droit sur l'icône de MMail dans la barre des tâches. Écrite à chaque
// démarrage, elle suit l'emplacement de l'exécutable.
inline void poserListeRaccourcis() {
  // PKEY_Title, écrite ici plutôt que tirée de propsys.lib.
  static const PROPERTYKEY titre = {{0xF29F85E0, 0x4FF9, 0x1068, {0xAB, 0x91, 0x08, 0x00, 0x2B, 0x27, 0xB3, 0xD9}}, 2};
  const HRESULT com = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
  ICustomDestinationList* liste = nullptr;
  if (SUCCEEDED(CoCreateInstance(CLSID_DestinationList, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&liste)))) {
    UINT places = 0;
    IObjectArray* retires = nullptr;
    if (SUCCEEDED(liste->BeginList(&places, IID_PPV_ARGS(&retires)))) {
      IObjectCollection* taches = nullptr;
      if (SUCCEEDED(CoCreateInstance(CLSID_EnumerableObjectCollection, nullptr, CLSCTX_INPROC_SERVER,
                                     IID_PPV_ARGS(&taches)))) {
        IShellLinkW* lien = nullptr;
        if (SUCCEEDED(CoCreateInstance(CLSID_ShellLink, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&lien)))) {
          wchar_t chemin[MAX_PATH] = {};
          GetModuleFileNameW(nullptr, chemin, MAX_PATH);
          lien->SetPath(chemin);
          lien->SetArguments(L"--ramener");
          lien->SetIconLocation(chemin, 0);
          lien->SetDescription(L"Ramener la fenêtre de MMail au centre de l'écran principal");
          IPropertyStore* proprietes = nullptr;
          if (SUCCEEDED(lien->QueryInterface(IID_PPV_ARGS(&proprietes)))) {
            const wchar_t libelle[] = L"Ramener la fenêtre";
            PROPVARIANT valeur;
            PropVariantInit(&valeur);
            valeur.vt = VT_LPWSTR;
            valeur.pwszVal = static_cast<LPWSTR>(CoTaskMemAlloc(sizeof(libelle)));
            if (valeur.pwszVal) {
              memcpy(valeur.pwszVal, libelle, sizeof(libelle));
              proprietes->SetValue(titre, valeur);
              proprietes->Commit();
            }
            PropVariantClear(&valeur);
            proprietes->Release();
          }
          taches->AddObject(lien);
          lien->Release();
        }
        IObjectArray* tableau = nullptr;
        if (SUCCEEDED(taches->QueryInterface(IID_PPV_ARGS(&tableau)))) {
          liste->AddUserTasks(tableau);
          tableau->Release();
        }
        taches->Release();
      }
      liste->CommitList();
      if (retires)
        retires->Release();
    }
    liste->Release();
  }
  if (SUCCEEDED(com))
    CoUninitialize();
}
#endif

// Matériau translucide de la fenêtre, dessiné par le compositeur de Windows
// 11 : Mica prend le fond du bureau, Acrylique prend ce qui est derrière la
// fenêtre. Rien à peindre côté application, donc aucun coût d'affichage ;
// en contrepartie le matériau vaut pour la fenêtre entière et n'apparaît que
// là où rien ne peint par-dessus — chez MMail, le bandeau de l'en-tête rendu
// partiellement transparent, et les bords.
//
// Rend VRAI seulement si DWM relit la valeur qu'on vient de poser. Un appel
// DwmSetWindowAttribute qui échoue est silencieux : sans relecture, on
// croirait avoir posé le matériau sur un poste qui l'ignore. Faux sur
// Windows 10, où l'attribut n'existe pas, et ailleurs que sous Windows.
inline bool materiau(QQuickWindow* f, const QString& nom) {
#ifdef Q_OS_WIN
  if (!f)
    return false;
  // 1 = aucun, 2 = Mica, 3 = Acrylique, 4 = Mica par onglets.
  int voulu = 0;
  if (nom == QStringLiteral("mica"))
    voulu = 2;
  else if (nom == QStringLiteral("acrylique"))
    voulu = 3;
  if (voulu == 0)
    return false;
  const HWND poignee = reinterpret_cast<HWND>(f->winId());
  if (!poignee)
    return false;
  // DWMWA_USE_IMMERSIVE_DARK_MODE : le cadre et la barre de titre suivent le
  // bandeau sombre, sans quoi une barre claire surmonte une fenêtre nuit.
  BOOL sombre = TRUE;
  DwmSetWindowAttribute(poignee, 20, &sombre, sizeof(sombre));
  // DWMWA_SYSTEMBACKDROP_TYPE, à partir de Windows 11 22621.
  if (FAILED(DwmSetWindowAttribute(poignee, 38, &voulu, sizeof(voulu))))
    return false;
  int relu = 0;
  if (FAILED(DwmGetWindowAttribute(poignee, 38, &relu, sizeof(relu))))
    return false;
  return relu == voulu;
#else
  Q_UNUSED(f)
  Q_UNUSED(nom)
  return false;
#endif
}

}  // namespace fenetre
