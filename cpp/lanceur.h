// SPDX-License-Identifier: GPL-3.0-or-later
//
// Ouvre un fichier avec un logiciel désigné : les pièces jointes s'ouvrent
// avec le logiciel choisi par type de fichier, sans dépendre des associations
// du système (décision 9). QML ne sait pas lancer un programme.
//
// Le choix — quel logiciel pour quelle extension — est gardé par l'interface,
// sur ce poste seulement : un chemin de programme ne vaut que sur la machine
// où il a été désigné (décision 18).
#pragma once

#include <QtCore/QFileInfo>
#include <QtCore/QObject>
#include <QtCore/QProcess>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QUrl>

#ifdef Q_OS_WIN
#include <windows.h>
#include <shlwapi.h>

#include <string>
#endif

class Lanceur : public QObject
{
  Q_OBJECT
public:
  explicit Lanceur(QObject* parent = nullptr)
    : QObject(parent)
  {
  }

  /// Lance `programme` sur `fichier`, détaché : MMail ne l'attend pas. L'un et
  /// l'autre en chemin ou en adresse `file:`. Faux si le programme n'existe
  /// pas, n'est pas exécutable, ou n'a pas pu démarrer.
  Q_INVOKABLE bool lancer(const QString& programme, const QString& fichier) const
  {
    const QString exe = chemin_(programme);
    const QFileInfo info(exe);
    if (exe.isEmpty() || !info.isFile() || !info.isExecutable())
      return false;
    return QProcess::startDetached(exe, QStringList{ chemin_(fichier) });
  }

  /// Nom lisible d'un programme : « C:/…/Acrobat.exe » → « Acrobat ».
  Q_INVOKABLE QString nom(const QString& programme) const
  {
    return QFileInfo(chemin_(programme)).completeBaseName();
  }

  /// Chemin local d'une adresse `file:` (un chemin est rendu tel quel).
  Q_INVOKABLE QString chemin(const QString& adresse) const { return chemin_(adresse); }

  /// Adresse `file:` d'un chemin local, pour Qt.openUrlExternally : « # » ou
  /// « % » dans un nom de fichier y sont encodés.
  Q_INVOKABLE QUrl adresse(const QString& chemin) const { return QUrl::fromLocalFile(chemin_(chemin)); }

  /// Vrai si le système sait ouvrir seul les fichiers de cette extension.
  /// Sous Windows, faux quand il demanderait « Comment voulez-vous ouvrir ce
  /// fichier ? » : le logiciel choisi dans cette fenêtre échappe à MMail, qui
  /// ne peut donc pas le retenir. Ailleurs, toujours vrai : le système décide.
  Q_INVOKABLE bool associe(const QString& extension) const
  {
#ifdef Q_OS_WIN
    if (extension.isEmpty())
      return false;
    const std::wstring ext = L"." + extension.toStdWString();
    // Une commande, ou un verbe délégué : c'est ainsi que s'ouvrent les
    // applications du Store (Photos, par exemple). Sans pszOut, l'appel rend
    // S_FALSE et la taille du texte quand l'association existe.
    for (const ASSOCSTR quoi : { ASSOCSTR_COMMAND, ASSOCSTR_DELEGATEEXECUTE }) {
      DWORD taille = 0;
      if (AssocQueryStringW(ASSOCF_INIT_IGNOREUNKNOWN, quoi, ext.c_str(), nullptr, nullptr,
                            &taille) == S_FALSE
          && taille > 1)
        return true;
    }
    return false;
#else
    Q_UNUSED(extension);
    return true;
#endif
  }

private:
  static QString chemin_(const QString& adresse)
  {
    return adresse.startsWith(QLatin1String("file:")) ? QUrl(adresse).toLocalFile() : adresse;
  }
};
