# SPDX-License-Identifier: GPL-3.0-or-later
# MMail — noyau Rust (core/), interface Qt 6 / QML, coffre des mots de passe natif.
cmake_minimum_required(VERSION 3.24)
project(mmail VERSION 0.6.3 LANGUAGES CXX)

if(CMAKE_CXX_COMPILER_ID STREQUAL "MSVC")
  set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreadedDLL")
endif()

set(CMAKE_AUTOMOC ON)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# QuickDialogs2 sert les boîtes Ouvrir / Enregistrer / Message, LabsSettings la
# persistance de la géométrie de la fenêtre (module QtCore QML à partir de 6.5,
# que la machine de développement en 6.4 n'a pas). Network : le canal local par
# lequel une seconde instance réveille la première (Qml en dépend déjà, il est
# donc livré de toute façon).
find_package(Qt6 6.4 REQUIRED COMPONENTS Core Gui Network Qml Quick QuickControls2 QuickDialogs2 LabsSettings)
if(NOT ANDROID)
  find_package(Qt6 REQUIRED COMPONENTS QmlImportScanner)
endif()
qt_standard_project_setup()

if(MSVC)
  set_property(TARGET Qt6::Core Qt6::Gui Qt6::Network Qt6::Qml Qt6::Quick Qt6::QuickControls2
                      Qt6::QuickDialogs2 Qt6::LabsSettings
               PROPERTY MAP_IMPORTED_CONFIG_DEBUG "RELEASE")
endif()

if(ANDROID)
  if(ANDROID_ABI STREQUAL "arm64-v8a")
    set(Rust_CARGO_TARGET aarch64-linux-android)
  elseif(ANDROID_ABI STREQUAL "x86_64")
    set(Rust_CARGO_TARGET x86_64-linux-android)
  endif()
endif()

find_package(CxxQt QUIET)
if(NOT CxxQt_FOUND)
  include(FetchContent)
  FetchContent_Declare(CxxQt
    GIT_REPOSITORY https://github.com/kdab/cxx-qt-cmake.git
    GIT_TAG 0.10.0)
  FetchContent_MakeAvailable(CxxQt)
endif()

# En compilation croisée, Qt::qmake désigne le qmake de l'hôte : cxx-qt lirait alors les
# en-têtes du Qt de bureau. Sous Android, on lui donne le qmake du Qt cible.
set(MMAIL_QMAKE)
if(ANDROID)
  set(MMAIL_QMAKE QMAKE "${QT6_INSTALL_PREFIX}/${QT6_INSTALL_BINS}/qmake")
endif()

cxx_qt_import_crate(
  MANIFEST_PATH core/Cargo.toml
  ${MMAIL_QMAKE}
  CRATES mmail_core
  LOCKED
  QT_MODULES Qt::Core Qt::Gui Qt::Qml Qt::Quick Qt::QuickControls2)

cxx_qt_import_qml_module(mmail_qml_module
  URI "fr.mmedia.mmail"
  SOURCE_CRATE mmail_core)


# ------------------------------------------------------------------ coffre ---
include(FetchContent)
# Coffre des mots de passe (décision 18) : QtKeychain, compilé avec le programme.
#
# Ses sources sont reprises sans passer par son propre CMakeLists : celui-ci
# exige les modules ECM et interroge qmake ou qtpaths pour écrire un fichier
# .pri, ce qui ne tient pas en compilation croisée Android. On garde ses fichiers
# et on décide ici, par plateforme, lesquels compiler — c'est tout ce que fait
# son CMakeLists, moins l'installation.
FetchContent_Declare(qtkeychain
  GIT_REPOSITORY https://github.com/frankosterfeld/qtkeychain.git
  GIT_TAG 0.15.0
  # Répertoire inexistant : les sources sont récupérées, sans add_subdirectory.
  SOURCE_SUBDIR sans-cmake)
FetchContent_MakeAvailable(qtkeychain)
set(QTK "${qtkeychain_SOURCE_DIR}/qtkeychain")

set(COFFRE_SOURCES ${QTK}/keychain.cpp ${QTK}/keychain.h ${QTK}/keychain_p.h)
if(WIN32)
  list(APPEND COFFRE_SOURCES ${QTK}/keychain_win.cpp)
elseif(ANDROID)
  list(APPEND COFFRE_SOURCES ${QTK}/keychain_android.cpp ${QTK}/androidkeystore.cpp
                             ${QTK}/androidkeystore_p.h ${QTK}/plaintextstore.cpp
                             ${QTK}/plaintextstore_p.h)
elseif(APPLE)
  enable_language(OBJCXX)
  list(APPEND COFFRE_SOURCES ${QTK}/keychain_apple.mm)
else()
  # Secret Service par libsecret, KWallet par D-Bus : QtKeychain choisit à
  # l'exécution selon le bureau.
  find_package(Qt6 REQUIRED COMPONENTS DBus)
  find_package(PkgConfig REQUIRED)
  pkg_check_modules(LIBSECRET REQUIRED IMPORTED_TARGET libsecret-1)
  qt6_add_dbus_interface(COFFRE_SOURCES ${QTK}/org.kde.KWallet.xml kwallet_interface KWalletInterface)
  list(APPEND COFFRE_SOURCES ${QTK}/keychain_unix.cpp ${QTK}/gnomekeyring.cpp
                             ${QTK}/gnomekeyring_p.h ${QTK}/libsecret.cpp ${QTK}/libsecret_p.h
                             ${QTK}/plaintextstore.cpp ${QTK}/plaintextstore_p.h)
endif()

add_library(coffre STATIC ${COFFRE_SOURCES})
set_target_properties(coffre PROPERTIES AUTOMOC ON)
# Les en-têtes s'incluent en <qtkeychain/keychain.h> ; pas de bibliothèque
# partagée, donc pas de macro d'exportation à générer.
target_include_directories(coffre PUBLIC ${qtkeychain_SOURCE_DIR}
                                  PRIVATE ${QTK} ${CMAKE_CURRENT_BINARY_DIR})
target_compile_definitions(coffre PUBLIC QTKEYCHAIN_NO_EXPORT)
target_link_libraries(coffre PUBLIC Qt::Core)
if(WIN32)
  target_compile_definitions(coffre PRIVATE USE_CREDENTIAL_STORE=1 UNICODE _UNICODE)
  target_link_libraries(coffre PRIVATE crypt32 advapi32)
elseif(APPLE)
  target_link_libraries(coffre PRIVATE "-framework Foundation" "-framework Security")
elseif(NOT ANDROID)
  target_compile_definitions(coffre PRIVATE KEYCHAIN_DBUS=1 HAVE_LIBSECRET=1)
  target_link_libraries(coffre PRIVATE Qt6::DBus PkgConfig::LIBSECRET)
endif()

# L'icône sert deux fois : à la fenêtre (ressource Qt, toutes plateformes) et à
# l'exécutable lui-même sous Windows, où elle passe par un fichier de ressources
# — sans quoi l'explorateur et les raccourcis affichent une icône générique.
# Le .h figure dans les sources pour qu'AUTOMOC le traite : sans cela, la classe
# QObject qu'il declare n'a pas de metaobjet et l'edition de liens echoue.
set(MMAIL_SOURCES cpp/main.cpp cpp/coffre.h cpp/fenetre.h cpp/installeur.h cpp/lanceur.h cpp/pastille.h cpp/notifieur.h cpp/impression.h cpp/presse_papier.h
                  cpp/mise_en_forme.h)
if(WIN32)
  # Icône et informations de version de l'exécutable (cf. build/mmail.rc.in).
  configure_file(build/mmail.rc.in ${CMAKE_CURRENT_BINARY_DIR}/mmail.rc @ONLY)
  list(APPEND MMAIL_SOURCES ${CMAKE_CURRENT_BINARY_DIR}/mmail.rc)
endif()

# Traductions françaises de Qt lui-même — boutons standard des dialogues
# (« OK », « Annuler », « Fermer »…), boîtes de fichiers —, embarquées dans
# l'exécutable : sans elles, ces boutons restaient en anglais (constaté sous
# Android le 18/10/2026, vrai partout). Lues dans les traductions de Qt de la
# fabrication, ou du Qt hôte pour Android.
set(MMAIL_QM_DIRS "${QT6_INSTALL_PREFIX}/${QT6_INSTALL_TRANSLATIONS}")
if(QT_HOST_PATH)
  list(APPEND MMAIL_QM_DIRS "${QT_HOST_PATH}/translations")
endif()
list(APPEND MMAIL_QM_DIRS /usr/share/qt6/translations)
set(MMAIL_QM_CONTENU "// Généré par CMake (CMakeLists.txt) : traductions de Qt embarquées.\n#include <cstddef>\n")
set(MMAIL_QM_LISTE "")
foreach(nom qtbase_fr qtdeclarative_fr)
  set(trouve "")
  foreach(d IN LISTS MMAIL_QM_DIRS)
    if(NOT trouve AND EXISTS "${d}/${nom}.qm")
      set(trouve "${d}/${nom}.qm")
    endif()
  endforeach()
  if(trouve)
    file(READ "${trouve}" octets HEX)
    string(REGEX REPLACE "([0-9a-f][0-9a-f])" "0x\\1," octets "${octets}")
    string(APPEND MMAIL_QM_CONTENU "static const unsigned char ${nom}[] = {${octets}};\n")
    list(APPEND MMAIL_QM_LISTE "{${nom}, sizeof(${nom})}")
    message(STATUS "Traduction de Qt embarquée : ${trouve}")
  else()
    message(WARNING "Traduction de Qt introuvable : ${nom}.qm (boutons standard en anglais)")
  endif()
endforeach()
list(LENGTH MMAIL_QM_LISTE MMAIL_QM_NOMBRE)
list(APPEND MMAIL_QM_LISTE "{nullptr, 0}")
list(JOIN MMAIL_QM_LISTE ", " MMAIL_QM_LISTE)
string(APPEND MMAIL_QM_CONTENU "struct TraductionQt { const unsigned char* donnees; std::size_t taille; };\n"
  "extern const TraductionQt traductionsQt[] = {${MMAIL_QM_LISTE}};\n"
  "extern const std::size_t nombreTraductionsQt = ${MMAIL_QM_NOMBRE};\n")
file(CONFIGURE OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/traductions_qt.cpp" CONTENT "${MMAIL_QM_CONTENU}" @ONLY)
list(APPEND MMAIL_SOURCES "${CMAKE_CURRENT_BINARY_DIR}/traductions_qt.cpp")

qt_add_executable(mmail ${MMAIL_SOURCES})
# Pas de ressource Qt sous Android : le rcc de l'hote compresse en zstd, absent
# du Qt officiel pour Android, et l'edition de liens s'arrete sur « undefined
# symbol: qt_resourceFeatureZstd » (--no-zstd n'y a rien change). L'icone ne sert
# de toute facon qu'au bureau ; sous Android, c'est le manifeste qui la porte. Le
# PNG est le signe du bandeau de l'apparence MMEDIA, qui s'en passe sous Android.
if(NOT ANDROID)
  qt_add_resources(mmail "ressources"
    PREFIX "/"
    FILES assets/mmail.ico assets/mmail.png)
endif()
target_link_libraries(mmail PRIVATE mmail_qml_module Qt::Network Qt::Quick Qt::QuickControls2
                                      Qt::QuickDialogs2 Qt::LabsSettings coffre)
target_compile_definitions(mmail PRIVATE MMAIL_VERSION="${PROJECT_VERSION}")

# Liste de raccourcis de la barre des tâches (cf. cpp/fenetre.h) : COM et
# interfaces du shell. shlwapi : associations des types de fichiers (cf.
# cpp/lanceur.h).
if(WIN32)
  target_link_libraries(mmail PRIVATE ole32 shell32 shlwapi uuid)
endif()

# Compteur du lanceur sous Linux (cf. cpp/pastille.h) : un signal D-Bus.
if(NOT WIN32 AND NOT APPLE AND NOT ANDROID)
  target_link_libraries(mmail PRIVATE Qt6::DBus)
endif()

# Le vérificateur de certificats de la plateforme s'appuie sur le Security
# Framework sous macOS : sans ces deux bibliothèques, l'édition de liens
# s'arrête sur « _kSecValueRef, symbol(s) not found ».
if(APPLE)
  target_link_libraries(mmail PRIVATE "-framework Security" "-framework CoreFoundation")
endif()

set_target_properties(mmail PROPERTIES
  WIN32_EXECUTABLE ON
  MACOSX_BUNDLE ON
  MACOSX_BUNDLE_GUI_IDENTIFIER fr.mmedia.mmail
  MACOSX_BUNDLE_BUNDLE_NAME MMail
  MACOSX_BUNDLE_BUNDLE_VERSION ${PROJECT_VERSION}
  MACOSX_BUNDLE_SHORT_VERSION_STRING ${PROJECT_VERSION}
  QT_ANDROID_PACKAGE_NAME fr.mmedia.mmail
  QT_ANDROID_APP_NAME MMail
  QT_ANDROID_VERSION_NAME ${PROJECT_VERSION})

if(NOT ANDROID)
  qt_import_qml_plugins(mmail)
endif()

# Le client parle au réseau par ses propres sockets, et non par Qt Network :
# rien ne garantit que la permission soit déduite des modules liés. Elle est
# donc déclarée ici (commande apparue avec Qt 6.8, celui de la fabrication
# Android).
# Sous Android, la connexion Microsoft se fait dans une page intégrée à MMail
# (QtWebView) : le navigateur du système, lui, laisse MMail gelé en arrière-plan.
if(ANDROID)
  find_package(Qt6 REQUIRED COMPONENTS WebView)
  target_link_libraries(mmail PRIVATE Qt6::WebView)
else()
  # Impression d'un message (cf. cpp/impression.h) : la boîte d'impression du
  # système, qui demande QtWidgets.
  find_package(Qt6 REQUIRED COMPONENTS Widgets PrintSupport)
  target_link_libraries(mmail PRIVATE Qt6::Widgets Qt6::PrintSupport)
  target_compile_definitions(mmail PRIVATE MMAIL_IMPRESSION)
endif()

if(ANDROID AND COMMAND qt_add_android_permission)
  qt_add_android_permission(mmail NAME android.permission.INTERNET)
  qt_add_android_permission(mmail NAME android.permission.ACCESS_NETWORK_STATE)
endif()

if(NOT ANDROID)
  install(TARGETS mmail
    BUNDLE DESTINATION .
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
endif()

enable_testing()
add_test(NAME smoke COMMAND mmail --smoke)
set_tests_properties(smoke PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
