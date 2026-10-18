// SPDX-License-Identifier: GPL-3.0-or-later
// Q_OS_ANDROID n’est pas encore défini ici, avant les en-têtes de Qt.
#ifdef __ANDROID__
#include <QtWebView/QtWebView>
#endif
#include <cstddef>
#include <QtCore/QCoreApplication>
#include <QtCore/QCryptographicHash>
#include <QtCore/QDir>
#include <QtCore/QFileInfo>
#include <QtCore/QLockFile>
#include <QtCore/QRegularExpression>
#include <QtCore/QSettings>
#include <QtCore/QStandardPaths>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtCore/QSysInfo>
#include <QtCore/QTimer>
#include <QtCore/QTranslator>
#include <QtCore/QUrl>
#include <QtCore/QVariant>
#include <QtCore/QVariantMap>
#include <QtGui/QFont>
#include <QtGui/QGuiApplication>
#ifdef MMAIL_IMPRESSION
#include <QtWidgets/QApplication>
#endif
#include <QtGui/QIcon>
#if QT_CONFIG(sessionmanager)
#  include <QtGui/QSessionManager>
#endif
#include <QtNetwork/QLocalServer>
#include <QtNetwork/QLocalSocket>
#include <QtQml/QQmlApplicationEngine>
#include <QtQml/QQmlContext>
#include <QtQuickControls2/QQuickStyle>
#include <QtQuick/QQuickWindow>
#include <QtQml/qqml.h>

#include "coffre.h"
#include "fenetre.h"
#include "installeur.h"
#include "lanceur.h"
#include "pastille.h"

// Traductions françaises de Qt, embarquées (cf. CMakeLists.txt).
struct TraductionQt { const unsigned char* donnees; std::size_t taille; };
extern const TraductionQt traductionsQt[];
extern const std::size_t nombreTraductionsQt;
#include "impression.h"
#include "notifieur.h"
#include "presse_papier.h"
#include "mise_en_forme.h"

#ifdef Q_OS_WIN
#  include <windows.h>  // GetCommandLineW / CommandLineToArgvW
#  include <shellapi.h>
#endif

#include <cstdio>

/// Arguments du programme, en Unicode et sans perte.
///
/// Sous Windows, `argv` est encodé dans la page de code ANSI : la seule source
/// fiable est la ligne de commande native. Leçon prise sur MMdedit.
static QStringList argumentsUnicode()
{
#ifdef Q_OS_WIN
  int nombre = 0;
  LPWSTR* natifs = CommandLineToArgvW(GetCommandLineW(), &nombre);
  if (!natifs)
    return QCoreApplication::arguments();
  QStringList liste;
  liste.reserve(nombre);
  for (int i = 0; i < nombre; ++i)
    liste.append(QString::fromWCharArray(natifs[i]));
  LocalFree(natifs);
  return liste;
#else
  return QCoreApplication::arguments();
#endif
}

/// Réglage de la machine, pour un serveur partagé (RDS) : la variable
/// d'environnement l'emporte, puis la base de registre de la machine
/// (HKLM\Software\M-Media\MMail sous Windows, /etc/xdg/M-Media/MMail.conf
/// ailleurs). Vide s'il n'y en a pas.
static QString reglageMachine(const char* variable, const QString& cle)
{
  const QString valeur = qEnvironmentVariable(variable);
  if (!valeur.isEmpty())
    return valeur;
  QSettings machine(QSettings::NativeFormat, QSettings::SystemScope, QCoreApplication::organizationName(),
                    QCoreApplication::applicationName());
  return machine.value(cle).toString().trimmed();
}

/// Développe les variables d'un chemin réglé pour toute la machine :
/// « %USERNAME% » comme « $USER » ou « ${USER} ».
static QString developper(QString chemin)
{
  static const QRegularExpression variable(QStringLiteral(R"(%(\w+)%|\$\{(\w+)\}|\$(\w+))"));
  QString sortie;
  qsizetype debut = 0;
  for (auto it = variable.globalMatch(chemin); it.hasNext();) {
    const auto trouve = it.next();
    sortie += chemin.mid(debut, trouve.capturedStart() - debut);
    QString nom = trouve.captured(1);
    if (nom.isEmpty())
      nom = trouve.captured(2);
    if (nom.isEmpty())
      nom = trouve.captured(3);
    sortie += qEnvironmentVariable(nom.toUtf8().constData());
    debut = trouve.capturedEnd();
  }
  sortie += chemin.mid(debut);
  return QDir::cleanPath(sortie);
}

/// Dossier du profil, résolu par le système et jamais écrit en dur — sauf
/// réglage de la machine : sur un serveur RDS, un profil itinérant ou
/// redirigé partirait sur le réseau, où SQLite en WAL se comporte mal
/// (décision 18 du dossier de projet). L'administrateur pose alors
/// « DossierProfil », par exemple D:\MMail\%USERNAME%.
static QString dossierProfil()
{
  const QString impose = reglageMachine("MMAIL_DOSSIER_PROFIL", QStringLiteral("DossierProfil"));
  const QString base = impose.isEmpty() ? QStandardPaths::writableLocation(QStandardPaths::AppDataLocation)
                                        : developper(impose);
  QDir().mkpath(base);
  return base;
}

int main(int argc, char* argv[])
{
#ifdef Q_OS_ANDROID
  // Connexion Microsoft dans une page intégrée (cf. ConnexionIntegree.qml).
  QtWebView::initialize();
#endif
#ifdef MMAIL_IMPRESSION
  // La boîte d'impression du système (QtPrintSupport) demande une
  // QApplication (cf. impression.h).
  QApplication app(argc, argv);
#else
  QGuiApplication app(argc, argv);
#endif
  QCoreApplication::setApplicationName(QStringLiteral("MMail"));
  // Les boutons standard des dialogues (« OK », « Annuler »…) et les boîtes
  // de Qt en français, quelle que soit la langue du système : MMail est en
  // français.
  for (std::size_t i = 0; i < nombreTraductionsQt; ++i) {
    auto* traduction = new QTranslator(&app);
    if (traduction->load(traductionsQt[i].donnees, int(traductionsQt[i].taille)))
      QCoreApplication::installTranslator(traduction);
  }
  QCoreApplication::setOrganizationName(QStringLiteral("M-Media"));
  QCoreApplication::setOrganizationDomain(QStringLiteral("mmedia.fr"));
  QCoreApplication::setApplicationVersion(QStringLiteral(MMAIL_VERSION));
  QGuiApplication::setWindowIcon(QIcon(QStringLiteral(":/assets/mmail.ico")));

  // Jours de messages gardés sur le poste (31 par défaut, 0 : aucun), réglables
  // pour la machine : le noyau les lit dans l'environnement.
  const QString joursCache = reglageMachine("MMAIL_JOURS_CACHE", QStringLiteral("JoursCache"));
  if (!joursCache.isEmpty())
    qputenv("MMAIL_JOURS_CACHE", joursCache.toUtf8());

  // Système du poste et sa version, que le noyau donne au serveur IMAP en se
  // présentant (commande ID) : Qt les nomme sur toutes les cibles, Rust non.
  qputenv("MMAIL_SYSTEME", QSysInfo::prettyProductName().toUtf8());

  // Fusion : le seul style qui honore une palette sur les trois cibles livrées.
  QQuickStyle::setStyle(QStringLiteral("Fusion"));

  // Polices de Windows demandées par les messages HTML (Outlook : Calibri),
  // absentes sous Linux et Android : sans équivalent désigné, le repli du
  // système peut tomber sur une police à chasse fixe. Sans effet là où la
  // police existe.
  QFont::insertSubstitutions(QStringLiteral("Calibri"),
                             {QStringLiteral("Carlito"), QStringLiteral("Liberation Sans"),
                              QStringLiteral("DejaVu Sans"), QStringLiteral("Noto Sans"),
                              QStringLiteral("Roboto")});
  QFont::insertSubstitutions(QStringLiteral("Cambria"),
                             {QStringLiteral("Caladea"), QStringLiteral("Liberation Serif"),
                              QStringLiteral("DejaVu Serif"), QStringLiteral("Noto Serif")});
  for (const char* famille : {"Segoe UI", "Tahoma", "Verdana", "Helvetica", "Aptos"})
    QFont::insertSubstitutions(QString::fromLatin1(famille),
                               {QStringLiteral("Liberation Sans"), QStringLiteral("DejaVu Sans"),
                                QStringLiteral("Noto Sans"), QStringLiteral("Roboto")});
  // Familles génériques de CSS : le moteur de texte riche les cherche comme
  // des polices ordinaires. Le noyau termine par « sans-serif » toute liste
  // de polices d'un message qui n'en a pas ; chacune désigne ici des polices
  // présentes sur l'une ou l'autre cible.
  QFont::insertSubstitutions(QStringLiteral("sans-serif"),
                             {QStringLiteral("Liberation Sans"), QStringLiteral("DejaVu Sans"),
                              QStringLiteral("Noto Sans"), QStringLiteral("Roboto"), QStringLiteral("Arial")});
  QFont::insertSubstitutions(QStringLiteral("serif"),
                             {QStringLiteral("Liberation Serif"), QStringLiteral("DejaVu Serif"),
                              QStringLiteral("Noto Serif"), QStringLiteral("Times New Roman")});
  QFont::insertSubstitutions(QStringLiteral("monospace"),
                             {QStringLiteral("Liberation Mono"), QStringLiteral("DejaVu Sans Mono"),
                              QStringLiteral("Noto Sans Mono"), QStringLiteral("Consolas"),
                              QStringLiteral("Courier New")});

  bool smoke = false;
  QString capture;
  int delaiCapture = 600;
  bool ramener = false;
  const QStringList arguments = argumentsUnicode();
  for (int i = 1; i < arguments.size(); ++i) {
    const QString argument = arguments.at(i);
    if (argument == QStringLiteral("--smoke"))
      smoke = true;
    // Clic droit sur la barre des tâches, « Ramener la fenêtre » (cf. fenetre.h).
    else if (argument == QStringLiteral("--ramener"))
      ramener = true;
    else if (argument == QStringLiteral("--capture") && i + 1 < arguments.size())
      capture = arguments.at(++i);
    // Délai avant la capture, en millisecondes : le temps qu'une session
    // d'essai se connecte et charge un dossier.
    else if (argument == QStringLiteral("--delai") && i + 1 < arguments.size())
      delaiCapture = arguments.at(++i).toInt();
  }

  // Coffre des mots de passe, natif : exposé à QML comme un type à part, hors du
  // module Rust.
  qmlRegisterType<Coffre>("fr.mmedia.mmail.natif", 1, 0, "Coffre");
  // Presse-papier du système, que QML ne sait pas lire : la copie automatique
  // de la sélection en a besoin pour ne pas écraser ce qu'un tiers y a déposé.
  qmlRegisterType<PressePapier>("fr.mmedia.mmail.natif", 1, 0, "PressePapier");
  // Lancement d'un logiciel choisi sur une pièce jointe (décision 9).
  qmlRegisterType<Lanceur>("fr.mmedia.mmail.natif", 1, 0, "Lanceur");
  // Nombre de non-lus des boîtes de réception sur l'icône de la barre des tâches.
  qmlRegisterType<Pastille>("fr.mmedia.mmail.natif", 1, 0, "Pastille");
  qmlRegisterType<Notifieur>("fr.mmedia.mmail.natif", 1, 0, "Notifieur");
  qmlRegisterType<Impression>("fr.mmedia.mmail.natif", 1, 0, "Impression");
  // Installation d'une mise à jour à la fermeture (cf. core/src/mise_a_jour.rs).
  qmlRegisterType<Installeur>("fr.mmedia.mmail.natif", 1, 0, "Installeur");
  // Mise en forme du message en cours de rédaction (gras, listes, liens…).
  qmlRegisterType<MiseEnForme>("fr.mmedia.mmail.natif", 1, 0, "MiseEnForme");

  QQmlApplicationEngine engine;
  // En contrôle de fabrication, l'interface ne touche ni au coffre ni au réseau
  // de sa propre initiative.
  engine.rootContext()->setContextProperty(QStringLiteral("modeControle"), smoke);
  // Avis de nouvelle version : coupé pour la machine (« AvisVersion » à 0) sur
  // un parc dont l'administrateur déploie lui-même les mises à jour.
  engine.rootContext()->setContextProperty(
    QStringLiteral("avisVersion"),
    reglageMachine("MMAIL_AVIS_VERSION", QStringLiteral("AvisVersion")) != QStringLiteral("0"));
  // Version de Qt réellement chargée, pour « À propos » : elle diffère d'une
  // cible à l'autre, et c'est la première chose à demander sur un défaut
  // d'affichage.
  engine.rootContext()->setContextProperty(QStringLiteral("versionQt"),
                                           QString::fromLatin1(qVersion()));
  // Commodité de développement : des variables d'environnement ouvrent une ou
  // deux sessions au démarrage, ce qui permet de saisir l'interface en image
  // sans personne devant l'écran. Rien n'est lu si elles sont absentes.
  const QByteArray hoteEssai = qgetenv("MMAIL_HOTE");
  QVariantMap essai;
  if (!hoteEssai.isEmpty()) {
    essai.insert(QStringLiteral("hote"), QString::fromUtf8(hoteEssai));
    essai.insert(QStringLiteral("utilisateur"), QString::fromUtf8(qgetenv("MMAIL_UTILISATEUR")));
    essai.insert(QStringLiteral("motDePasse"), QString::fromUtf8(qgetenv("MMAIL_MOTDEPASSE")));
    essai.insert(QStringLiteral("utilisateur2"), QString::fromUtf8(qgetenv("MMAIL_UTILISATEUR2")));
    essai.insert(QStringLiteral("motDePasse2"), QString::fromUtf8(qgetenv("MMAIL_MOTDEPASSE2")));
    essai.insert(QStringLiteral("scenario"), QString::fromUtf8(qgetenv("MMAIL_SCENARIO")));
    essai.insert(QStringLiteral("dossier"), QString::fromUtf8(qgetenv("MMAIL_DOSSIER")));
    // Serveur d'envoi du premier compte, pour le scénario « envoi-fond ».
    essai.insert(QStringLiteral("hoteEnvoi"), QString::fromUtf8(qgetenv("MMAIL_HOTE_ENVOI")));
    // Vue tactile d'Android imposée sur le bureau (MMAIL_TACTILE=1) et taille
    // de la fenêtre (MMAIL_TAILLE=412x915) : la vue du téléphone en image,
    // sans téléphone.
    essai.insert(QStringLiteral("tactile"), qgetenv("MMAIL_TACTILE") == "1");
    essai.insert(QStringLiteral("taille"), QString::fromUtf8(qgetenv("MMAIL_TAILLE")));
    essai.insert(QStringLiteral("sortie"),
                 QUrl::fromLocalFile(QString::fromUtf8(qgetenv("MMAIL_SORTIE"))).toString());
  }
  engine.rootContext()->setContextProperty(QStringLiteral("identifiantsEssai"), essai);

  // Dossier proposé à l'enregistrement d'une pièce jointe : celui des
  // téléchargements de l'utilisateur, résolu par le système.
  const QString telechargements = QStandardPaths::writableLocation(QStandardPaths::DownloadLocation);
  engine.rootContext()->setContextProperty(QStringLiteral("dossierTelechargements"),
                                           QUrl::fromLocalFile(telechargements).toString());

  // MMAIL_PROFIL : un profil jetable pour les essais, qui ne touche pas celui
  // de l'utilisateur.
  const QByteArray profilEssai = qgetenv("MMAIL_PROFIL");
  const QString profil = profilEssai.isEmpty()
    ? QDir(dossierProfil()).filePath(QStringLiteral("index.sqlite"))
    : QString::fromUtf8(profilEssai);
  engine.rootContext()->setContextProperty(QStringLiteral("cheminProfil"), profil);

  // Un seul MMail par profil. Deux instances videraient l'une sous l'autre
  // les pièces jointes ouvertes, enverraient deux fois les messages différés
  // et rejoueraient ensemble les déplacements interrompus. Un double clic au
  // lancement suffisait. La seconde réveille la fenêtre de la première et
  // s'arrête. Le verrou d'un MMail arrêté net (processus disparu) est repris.
  QDir().mkpath(QFileInfo(profil).absolutePath());
  QLockFile verrou(QFileInfo(profil).absolutePath() + QStringLiteral("/mmail.lock"));
  verrou.setStaleLockTime(0);
  const QString canal = QStringLiteral("mmail-") +
    QString::fromLatin1(QCryptographicHash::hash(QFileInfo(profil).absoluteFilePath().toUtf8(),
                                                 QCryptographicHash::Sha1).toHex().left(16));
  if (!verrou.tryLock(0)) {
#ifdef Q_OS_WIN
    // Le premier plan revient à celui que l'utilisateur vient de lancer : il
    // le cède à l'instance déjà ouverte.
    AllowSetForegroundWindow(ASFW_ANY);
#endif
    QLocalSocket appel;
    appel.connectToServer(canal);
    if (appel.waitForConnected(1000)) {
      appel.write(ramener ? "ramener\n" : "montrer\n");
      appel.waitForBytesWritten(1000);
    }
    std::fprintf(stderr, "MMail est déjà ouvert sur ce profil.\n");
    return 0;
  }
  QLocalServer reveil;
  QLocalServer::removeServer(canal);
  reveil.listen(canal);
  // Le second lancement dit ce qu'il veut : « montrer » la fenêtre, ou la
  // « ramener » au centre de l'écran principal.
  QObject::connect(&reveil, &QLocalServer::newConnection, &app, [&reveil, &engine] {
    while (QLocalSocket* appel = reveil.nextPendingConnection()) {
      QObject::connect(appel, &QLocalSocket::disconnected, appel, &QObject::deleteLater);
      QObject::connect(appel, &QLocalSocket::readyRead, appel, [appel, &engine] {
        if (!appel->canReadLine() || engine.rootObjects().isEmpty())
          return;
        const QByteArray demande = appel->readLine().trimmed();
        auto* principale = qobject_cast<QQuickWindow*>(engine.rootObjects().first());
        if (!principale)
          return;
        if (demande == "ramener") {
          fenetre::ramener(principale);
          return;
        }
        if (principale->windowStates() & Qt::WindowMinimized)
          principale->showNormal();
        principale->show();
        principale->raise();
        principale->requestActivate();
      });
    }
  });

  const QUrl url(QStringLiteral("qrc:/qt/qml/fr/mmedia/mmail/qml/Main.qml"));
  QObject::connect(
    &engine, &QQmlApplicationEngine::objectCreated, &app,
    [url](QObject* obj, const QUrl& objUrl) {
      if (!obj && url == objUrl)
        QCoreApplication::exit(1);
    },
    Qt::QueuedConnection);
  engine.load(url);

#ifndef Q_OS_ANDROID
  // Position de la fenêtre retenue d'une session à l'autre — sauf pour les
  // contrôles, les captures et les essais à taille imposée (MMAIL_TAILLE).
  if (!smoke && capture.isEmpty() && qgetenv("MMAIL_TAILLE").isEmpty() && !engine.rootObjects().isEmpty()) {
    if (auto* principale = qobject_cast<QQuickWindow*>(engine.rootObjects().first())) {
      fenetre::suivre(principale);
      if (ramener)
        fenetre::ramener(principale);
    }
  }
#endif
#ifdef Q_OS_WIN
  fenetre::poserListeRaccourcis();
#endif

  if (smoke) {
    // Contrôle de fabrication : la fenêtre existe, le noyau Rust répond, et
    // l'index local s'ouvre et se relit. Rien de tout cela ne touche au réseau.
    QTimer::singleShot(0, &app, [&engine, profil] {
      if (engine.rootObjects().isEmpty()) {
        std::fprintf(stderr, "smoke: aucune fenetre creee\n");
        QCoreApplication::exit(1);
        return;
      }
      QObject* racine = engine.rootObjects().first();
      const QString noyau = racine->property("noyau").toString();
      QVariant controle;
      const bool appel =
        QMetaObject::invokeMethod(racine, "controleIndex", Q_RETURN_ARG(QVariant, controle));
      std::printf("smoke: %s | profil %s | index %s\n", noyau.toUtf8().constData(),
                  QFileInfo::exists(profil) ? "ecrit" : "absent",
                  controle.toString().toUtf8().constData());
      std::fflush(stdout);
      if (!noyau.startsWith(QStringLiteral("mmail_core"))) {
        QCoreApplication::exit(2);
        return;
      }
      QCoreApplication::exit(appel && controle.toString() == QStringLiteral("ok") ? 0 : 3);
    });
  }

  if (!capture.isEmpty()) {
    QTimer::singleShot(delaiCapture, &app, [&engine, capture] {
      if (engine.rootObjects().isEmpty()) {
        QCoreApplication::exit(1);
        return;
      }
      auto* fenetre = qobject_cast<QQuickWindow*>(engine.rootObjects().first());
      if (!fenetre) {
        QCoreApplication::exit(5);
        return;
      }
      QCoreApplication::exit(fenetre->grabWindow().save(capture) ? 0 : 6);
    });
  }
#if QT_CONFIG(sessionmanager)
  // Arrêt ou déconnexion : pas d'installation lancée maintenant (cf.
  // cpp/installeur.h).
  QObject::connect(&app, &QGuiApplication::commitDataRequest, &app,
                   [](QSessionManager&) { Installeur::sessionFinie(); });
#endif
  const int code = app.exec();
  // Mise à jour retenue pour la fermeture : son installeur part maintenant, et
  // attend la fin de ce processus pour remplacer les fichiers.
  Installeur::executer();
  return code;
}
