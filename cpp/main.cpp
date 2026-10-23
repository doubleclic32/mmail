// SPDX-License-Identifier: GPL-3.0-or-later
// Q_OS_ANDROID n’est pas encore défini ici, avant les en-têtes de Qt.
#ifdef __ANDROID__
#include <QtWebView/QtWebView>
#endif
#include <cstddef>
#include <QtCore/QCoreApplication>
#include <QtCore/QCryptographicHash>
#include <QtCore/QDir>
#include <QtCore/QFile>
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
#  include <aclapi.h>   // droits du profil imposé (reserverProfil)
#  include <dxgi.h>     // cartes graphiques de la machine (choisirRendu)
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

/// Vrai si la machine n'a aucune carte graphique : Direct3D n'y trouve que
/// des adaptateurs logiciels ou de Microsoft (« Microsoft Basic Render
/// Driver », « Remote Display Adapter » d'une session RDS).
static bool sansCarteGraphique()
{
#ifdef Q_OS_WIN
  IDXGIFactory1* fabrique = nullptr;
  if (FAILED(CreateDXGIFactory1(IID_PPV_ARGS(&fabrique))))
    return false;
  bool materiel = false;
  IDXGIAdapter1* adaptateur = nullptr;
  for (UINT i = 0; !materiel && fabrique->EnumAdapters1(i, &adaptateur) != DXGI_ERROR_NOT_FOUND; ++i) {
    DXGI_ADAPTER_DESC1 description;
    if (SUCCEEDED(adaptateur->GetDesc1(&description)))
      materiel = !(description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) && description.VendorId != 0x1414;
    adaptateur->Release();
  }
  fabrique->Release();
  return !materiel;
#else
  return false;
#endif
}

/// Rendu de l'interface. Sans carte graphique (serveur RDS, machine
/// virtuelle), Qt Quick passe par Direct3D 11, qui retombe sur WARP, le rendu
/// logiciel de Windows : trois à quatre fois plus de processeur que le moteur
/// logiciel de Qt Quick, mesuré sur un serveur RDS les 19 et 20/10/2026. MMail
/// prend alors ce dernier. « Rendu » (« logiciel » ou « materiel ») impose le
/// choix pour la machine ; QT_QUICK_BACKEND, s'il est posé, l'emporte sur tout.
/// Vrai si le rendu est logiciel.
static bool choisirRendu()
{
  const QByteArray impose = qgetenv("QT_QUICK_BACKEND");
  if (!impose.isEmpty())
    return impose == "software";
  const QString reglage = reglageMachine("MMAIL_RENDU", QStringLiteral("Rendu")).toLower();
  const bool materiel = reglage == QStringLiteral("materiel") || reglage == QStringLiteral("matériel");
  const bool logiciel = reglage == QStringLiteral("logiciel") || (!materiel && sansCarteGraphique());
  if (logiciel)
    QQuickWindow::setGraphicsApi(QSGRendererInterface::Software);
  return logiciel;
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
static QString dossierProfil(bool* impose = nullptr)
{
  const QString reglage = reglageMachine("MMAIL_DOSSIER_PROFIL", QStringLiteral("DossierProfil"));
  const QString base = reglage.isEmpty() ? QStandardPaths::writableLocation(QStandardPaths::AppDataLocation)
                                         : developper(reglage);
  QDir().mkpath(base);
  if (impose)
    *impose = !reglage.isEmpty();
  return base;
}

#ifdef Q_OS_WIN
/// Vrai si `sid` est l'utilisateur, SYSTEM ou le groupe des Administrateurs.
static bool sidAdmis(PSID sid, PSID utilisateur)
{
  if (EqualSid(sid, utilisateur))
    return true;
  return IsWellKnownSid(sid, WinLocalSystemSid) || IsWellKnownSid(sid, WinBuiltinAdministratorsSid);
}
#endif

/// Profil imposé par la machine (serveur RDS, « DossierProfil ») : réservé à
/// l'utilisateur, à SYSTEM et aux Administrateurs (0.6.4). Le dossier vit hors
/// du profil Windows, dont il ne reçoit pas la protection : selon les droits
/// du dossier parent, les autres utilisateurs du serveur liraient le cache des
/// messages, l'index et les pièces jointes. Un dossier déjà réservé n'est pas
/// touché ; sinon ses droits sont remplacés, sans héritage. Rend faux, avec le
/// motif, si l'utilisateur ne peut pas les poser.
static bool reserverProfil(const QString& dossier, QString* motif)
{
#ifdef Q_OS_WIN
  const std::wstring chemin = QDir::toNativeSeparators(dossier).toStdWString();
  HANDLE jeton = nullptr;
  if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &jeton)) {
    *motif = QStringLiteral("identité de la session illisible");
    return false;
  }
  DWORD taille = 0;
  GetTokenInformation(jeton, TokenUser, nullptr, 0, &taille);
  QByteArray tampon(int(taille), '\0');
  const bool lu = GetTokenInformation(jeton, TokenUser, tampon.data(), taille, &taille);
  CloseHandle(jeton);
  if (!lu) {
    *motif = QStringLiteral("identité de la session illisible");
    return false;
  }
  PSID utilisateur = reinterpret_cast<TOKEN_USER*>(tampon.data())->User.Sid;

  // Déjà réservé ? Toute entrée qui accorde un droit à un autre compte que
  // les trois admis le refuse — sauf « Créateur propriétaire » qui ne vaut que
  // pour les enfants (héritage seul).
  PACL dacl = nullptr;
  PSECURITY_DESCRIPTOR descripteur = nullptr;
  bool reserve = false;
  if (GetNamedSecurityInfoW(chemin.c_str(), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, nullptr, nullptr, &dacl,
                            nullptr, &descripteur) == ERROR_SUCCESS && dacl) {
    reserve = true;
    for (DWORD i = 0; i < dacl->AceCount && reserve; ++i) {
      ACE_HEADER* entete = nullptr;
      if (!GetAce(dacl, i, reinterpret_cast<void**>(&entete)) || entete->AceType != ACCESS_ALLOWED_ACE_TYPE)
        continue;
      PSID sid = &reinterpret_cast<ACCESS_ALLOWED_ACE*>(entete)->SidStart;
      if (IsWellKnownSid(sid, WinCreatorOwnerSid) && (entete->AceFlags & INHERIT_ONLY_ACE))
        continue;
      reserve = sidAdmis(sid, utilisateur);
    }
  }
  if (descripteur)
    LocalFree(descripteur);
  if (reserve)
    return true;

  // Utilisateur, SYSTEM et Administrateurs, contrôle total, hérité par tout
  // ce que le dossier contient ; l'héritage du parent est coupé.
  BYTE systeme[SECURITY_MAX_SID_SIZE], admins[SECURITY_MAX_SID_SIZE];
  DWORD tailleSysteme = sizeof(systeme), tailleAdmins = sizeof(admins);
  CreateWellKnownSid(WinLocalSystemSid, nullptr, systeme, &tailleSysteme);
  CreateWellKnownSid(WinBuiltinAdministratorsSid, nullptr, admins, &tailleAdmins);
  EXPLICIT_ACCESS_W acces[3] = {};
  PSID sids[3] = {utilisateur, systeme, admins};
  for (int i = 0; i < 3; ++i) {
    acces[i].grfAccessPermissions = GENERIC_ALL;
    acces[i].grfAccessMode = SET_ACCESS;
    acces[i].grfInheritance = SUB_CONTAINERS_AND_OBJECTS_INHERIT;
    acces[i].Trustee.TrusteeForm = TRUSTEE_IS_SID;
    acces[i].Trustee.ptstrName = static_cast<LPWSTR>(sids[i]);
  }
  PACL nouvelle = nullptr;
  if (SetEntriesInAclW(3, acces, nullptr, &nouvelle) != ERROR_SUCCESS) {
    *motif = QStringLiteral("liste de droits impossible à composer");
    return false;
  }
  const DWORD issue = SetNamedSecurityInfoW(const_cast<LPWSTR>(chemin.c_str()), SE_FILE_OBJECT,
                                            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                                            nullptr, nullptr, nouvelle, nullptr);
  LocalFree(nouvelle);
  if (issue != ERROR_SUCCESS) {
    *motif = QStringLiteral("droits refusés (erreur %1)").arg(issue);
    return false;
  }
  return true;
#else
  // Ailleurs : lecture et écriture au seul propriétaire.
  const bool pose = QFile::setPermissions(dossier, QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                                                     QFileDevice::ExeOwner);
  if (!pose)
    *motif = QStringLiteral("droits du dossier impossibles à poser");
  return pose;
#endif
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
  // Taille au-delà de laquelle un message n'est pas gardé (10 Mio par défaut,
  // 0 : sans plafond), de même.
  const QString tailleCache = reglageMachine("MMAIL_TAILLE_CACHE", QStringLiteral("TailleCache"));
  if (!tailleCache.isEmpty())
    qputenv("MMAIL_TAILLE_CACHE", tailleCache.toUtf8());
  // Avant la première fenêtre : le moteur de rendu ne change plus ensuite.
  const bool renduLogiciel = choisirRendu();

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
  // De même pour le moteur de rendu choisi (cf. choisirRendu).
  engine.rootContext()->setContextProperty(QStringLiteral("renduLogiciel"), renduLogiciel);
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
  bool profilImpose = false;
  const QString profil = profilEssai.isEmpty()
    ? QDir(dossierProfil(&profilImpose)).filePath(QStringLiteral("index.sqlite"))
    : QString::fromUtf8(profilEssai);
  if (profilImpose) {
    QString motif;
    if (!reserverProfil(QFileInfo(profil).absolutePath(), &motif)) {
      // Un profil lisible par les autres utilisateurs du serveur exposerait
      // leurs messages : MMail ne démarre pas, et dit pourquoi.
      const QString texte = QStringLiteral(
        "Le dossier de MMail (%1) est lisible par d'autres utilisateurs de cet ordinateur, et MMail ne peut pas "
        "le réserver à votre session : %2.\n\nDemandez à votre administrateur de le réserver à votre compte.")
        .arg(QDir::toNativeSeparators(QFileInfo(profil).absolutePath()), motif);
      std::fprintf(stderr, "MMail : %s\n", texte.toLocal8Bit().constData());
#ifdef Q_OS_WIN
      MessageBoxW(nullptr, reinterpret_cast<LPCWSTR>(texte.utf16()), L"MMail", MB_OK | MB_ICONERROR);
#endif
      return 2;
    }
  }
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
      // MMAIL_CAPTURE_FENETRE (essais, 0.6.16) : la dernière fenêtre ouverte
      // par-dessus la principale — fenêtre d'un message, rédaction.
      if (!qgetenv("MMAIL_CAPTURE_FENETRE").isEmpty()) {
        const auto fenetres = QGuiApplication::topLevelWindows();
        for (auto* autre : fenetres) {
          auto* rapide = qobject_cast<QQuickWindow*>(autre);
          if (rapide && rapide != fenetre && rapide->isVisible())
            fenetre = rapide;
        }
      }
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
