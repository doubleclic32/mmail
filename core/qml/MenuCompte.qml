// SPDX-License-Identifier: GPL-3.0-or-later
// Menu contextuel d’un compte de l’arborescence.
//
// Sorti de Main.qml en 0.5.16 (découpage) : les noms employés sans être
// déclarés ici (fenetre, boite, reglages…) sont ceux de Main.qml,
// où « MenuCompte { id: menuCompte } » l'instancie.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

Menu {
    id: menuCompte
    font.pointSize: fenetre.tailleColonne("arborescence")
    onAboutToShow: width = fenetre.largeurMenu(menuCompte)
    property int compte: 0
    property string adresse: ""
    property string hote: ""
    property string etat: ""
    function ouvrir(c, a, h, e, item, x, y) {
        compte = c; adresse = a; hote = h; etat = e
        if (item)
            popup(item, x, y)
        else
            popup()
    }
    MenuItem {
        text: qsTr("Modifier ce compte…")
        onTriggered: fenetre.modifierCompte(menuCompte.compte)
    }
    MenuItem {
        text: menuCompte.etat === "pret" ? qsTr("Se déconnecter") : qsTr("Se connecter…")
        onTriggered: {
            if (menuCompte.etat === "pret")
                boite.deconnecter(menuCompte.compte)
            else
                fenetre.connecterCompte({ id: menuCompte.compte, adresse: menuCompte.adresse,
                                          hote: menuCompte.hote })
        }
    }
    MenuItem {
        text: qsTr("Oublier le mot de passe")
        onTriggered: coffre.effacer(fenetre.cle(menuCompte.adresse, menuCompte.hote))
    }
    MenuSeparator {}
    // L'ordre des comptes se règle aussi en glissant leur en-tête.
    MenuItem {
        text: qsTr("Monter ce compte")
        onTriggered: boite.decalerCompte(menuCompte.compte, -1)
    }
    MenuItem {
        text: qsTr("Descendre ce compte")
        onTriggered: boite.decalerCompte(menuCompte.compte, 1)
    }
    MenuSeparator {}
    MenuItem {
        text: qsTr("Retirer ce compte de MMail…")
        onTriggered: dlgRetrait.ouvrir(menuCompte.compte, menuCompte.adresse, menuCompte.hote)
    }
}
