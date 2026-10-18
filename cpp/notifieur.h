// SPDX-License-Identifier: GPL-3.0-or-later
//
// Notification du nouveau courrier (0.6.2, accord de Manu du 18/10/2026 :
// Windows et Linux d'abord ; Android demande une relève en arrière-plan, à
// part). Le noyau signale les messages arrivés dans une boîte de réception,
// une fois les filtres passés (Boite.nouveauCourrier) ; l'interface appelle
// notifier() quand la fenêtre n'est pas au premier plan.
//
// Windows : bulle de la zone de notification (Shell_NotifyIcon, NIF_INFO),
// que Windows 10 et 11 montrent comme une notification ; l'icône de MMail
// reste dans la zone tant que MMail tourne, et un clic sur la bulle ou
// l'icône ramène la fenêtre. Linux : org.freedesktop.Notifications par
// D-Bus, action « Ouvrir ». Ailleurs, rien.
#pragma once

#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtGui/QImage>
#include <QtGui/QPixmap>

#ifdef Q_OS_WIN
#include <windows.h>
#include <shellapi.h>
#endif
#if defined(Q_OS_LINUX) && !defined(Q_OS_ANDROID)
#include <QtCore/QStringList>
#include <QtCore/QVariantMap>
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusMessage>
#include <QtDBus/QDBusReply>
#endif

class Notifieur : public QObject
{
  Q_OBJECT

public:
  explicit Notifieur(QObject* parent = nullptr) : QObject(parent)
  {
#if defined(Q_OS_LINUX) && !defined(Q_OS_ANDROID)
    QDBusConnection::sessionBus().connect(QStringLiteral("org.freedesktop.Notifications"),
                                          QStringLiteral("/org/freedesktop/Notifications"),
                                          QStringLiteral("org.freedesktop.Notifications"),
                                          QStringLiteral("ActionInvoked"), this,
                                          SLOT(actionInvoquee(uint, QString)));
#endif
  }

  ~Notifieur() override
  {
#ifdef Q_OS_WIN
    if (m_fenetre) {
      NOTIFYICONDATAW nid = donnees();
      Shell_NotifyIconW(NIM_DELETE, &nid);
      DestroyWindow(m_fenetre);
    }
    if (m_icone)
      DestroyIcon(m_icone);
#endif
  }

  /// Montre une notification ; rend faux si le système n'en a pas.
  Q_INVOKABLE bool notifier(const QString& titre, const QString& texte)
  {
#ifdef Q_OS_WIN
    if (!preparer())
      return false;
    NOTIFYICONDATAW nid = donnees();
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON;
    nid.hBalloonIcon = m_icone;
    copier(nid.szInfoTitle, sizeof(nid.szInfoTitle) / sizeof(wchar_t), titre);
    copier(nid.szInfo, sizeof(nid.szInfo) / sizeof(wchar_t), texte);
    return Shell_NotifyIconW(NIM_MODIFY, &nid) != FALSE;
#elif defined(Q_OS_LINUX) && !defined(Q_OS_ANDROID)
    QDBusMessage appel = QDBusMessage::createMethodCall(QStringLiteral("org.freedesktop.Notifications"),
                                                        QStringLiteral("/org/freedesktop/Notifications"),
                                                        QStringLiteral("org.freedesktop.Notifications"),
                                                        QStringLiteral("Notify"));
    QVariantMap indices;
    indices.insert(QStringLiteral("desktop-entry"), QStringLiteral("mmail"));
    indices.insert(QStringLiteral("category"), QStringLiteral("email.arrived"));
    appel << QStringLiteral("MMail") << m_dernier << QStringLiteral("mail-unread") << titre << texte
          << QStringList{QStringLiteral("default"), tr("Ouvrir")} << indices << -1;
    QDBusReply<uint> reponse = QDBusConnection::sessionBus().call(appel, QDBus::Block, 2000);
    if (!reponse.isValid())
      return false;
    m_dernier = reponse.value();
    return true;
#else
    Q_UNUSED(titre)
    Q_UNUSED(texte)
    return false;
#endif
  }

signals:
  /// La personne a cliqué la notification : ramener la fenêtre.
  void active();

private slots:
  void actionInvoquee(uint id, const QString& action)
  {
    Q_UNUSED(action)
    if (id == m_dernier)
      emit active();
  }

private:
  uint m_dernier = 0;

#ifdef Q_OS_WIN
  static constexpr UINT MESSAGE = WM_APP + 41;
  HWND m_fenetre = nullptr;
  HICON m_icone = nullptr;

  static void copier(wchar_t* cible, size_t taille, const QString& texte)
  {
    const std::wstring w = texte.left(int(taille) - 1).toStdWString();
    wcsncpy_s(cible, taille, w.c_str(), _TRUNCATE);
  }

  NOTIFYICONDATAW donnees() const
  {
    NOTIFYICONDATAW nid{};
    nid.cbSize = sizeof(nid);
    nid.hWnd = m_fenetre;
    nid.uID = 1;
    return nid;
  }

  static LRESULT CALLBACK procedure(HWND fenetre, UINT message, WPARAM w, LPARAM l)
  {
    if (message == MESSAGE) {
      const UINT quoi = LOWORD(l);
      if (quoi == NIN_BALLOONUSERCLICK || quoi == WM_LBUTTONUP || quoi == NIN_SELECT) {
        auto* moi = reinterpret_cast<Notifieur*>(GetWindowLongPtrW(fenetre, GWLP_USERDATA));
        if (moi)
          emit moi->active();
      }
      return 0;
    }
    return DefWindowProcW(fenetre, message, w, l);
  }

  // Fenêtre de messages invisible et icône de la zone de notification, à la
  // première notification.
  bool preparer()
  {
    if (m_fenetre)
      return true;
    const wchar_t* classe = L"MMailNotifieur";
    WNDCLASSW wc{};
    wc.lpfnWndProc = &Notifieur::procedure;
    wc.hInstance = GetModuleHandleW(nullptr);
    wc.lpszClassName = classe;
    RegisterClassW(&wc);
    m_fenetre = CreateWindowExW(0, classe, L"MMail", 0, 0, 0, 0, 0, HWND_MESSAGE, nullptr, wc.hInstance, nullptr);
    if (!m_fenetre)
      return false;
    SetWindowLongPtrW(m_fenetre, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(this));
    m_icone = QGuiApplication::windowIcon().pixmap(32, 32).toImage().toHICON();
    NOTIFYICONDATAW nid = donnees();
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = MESSAGE;
    nid.hIcon = m_icone ? m_icone : LoadIconW(nullptr, IDI_APPLICATION);
    copier(nid.szTip, sizeof(nid.szTip) / sizeof(wchar_t), QStringLiteral("MMail"));
    if (!Shell_NotifyIconW(NIM_ADD, &nid))
      return false;
    nid.uVersion = NOTIFYICON_VERSION_4;
    Shell_NotifyIconW(NIM_SETVERSION, &nid);
    return true;
  }
#endif
};
