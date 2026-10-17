// SPDX-License-Identifier: GPL-3.0-or-later
// Réglage posé seulement sous une apparence de marque (« MMEDIA »).
//
// Composant en ligne de Main.qml jusqu'à la 0.5.16 : sorti pour servir aussi
// aux fichiers découpés, un composant en ligne n'étant visible que du sien.
import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtQuick.Window
import Qt.labs.settings
import fr.mmedia.mmail
import fr.mmedia.mmail.natif

// Surcharge qui ne vaut que sous une apparence de marque : hors d'elle,
// la propriété retrouve sa valeur d'origine, celle du style.
Binding {
    when: fenetre.marque !== null
    restoreMode: Binding.RestoreBindingOrValue
}
