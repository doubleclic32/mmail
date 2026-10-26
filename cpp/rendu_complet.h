// SPDX-License-Identifier: GPL-3.0-or-later
//
// Rendu complet d'un texte riche long. Au-delà de 10 000 caractères, un
// TextEdit de Qt 6 ne dessine que les blocs proches de la vue, et ne redessine
// que lorsque la vue sort de la zone qu'il croit couverte. Dès que le texte
// porte un tableau (une signature, un message transféré), cette zone est mal
// tenue : descendu en bas d'un long message puis remonté, le corps restait
// blanc jusqu'à ce qu'on rouvre le message (constaté le 26/10/2026, Qt 6.8.3 ;
// même code dans les versions suivantes). Ce découpage est retiré : le texte
// est dessiné en entier, comme un texte court.
//
// « RenduComplet { zone: vueCorps } » dans le TextArea.
#pragma once

#include <QtCore/QMetaObject>
#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtQuick/QQuickItem>

class RenduComplet : public QObject
{
  Q_OBJECT
  Q_PROPERTY(QQuickItem* zone READ zone WRITE setZone NOTIFY zoneChange)

public:
  using QObject::QObject;

  QQuickItem* zone() const { return m_zone; }
  void setZone(QQuickItem* zone)
  {
    if (m_zone == zone)
      return;
    if (m_zone)
      disconnect(m_zone, nullptr, this, nullptr);
    m_zone = zone;
    if (m_zone) {
      // Différé : le TextEdit ne pose son découpage qu'après avoir annoncé son
      // nouveau texte (QQuickTextEdit::setText).
      connect(m_zone, SIGNAL(textChanged()), this, SLOT(retirerDecoupage()), Qt::QueuedConnection);
      retirerDecoupage();
    }
    Q_EMIT zoneChange();
  }

Q_SIGNALS:
  void zoneChange();

private Q_SLOTS:
  void retirerDecoupage()
  {
    if (!m_zone || !m_zone->flags().testFlag(QQuickItem::ItemObservesViewport))
      return;
    m_zone->setFlag(QQuickItem::ItemObservesViewport, false);
    // Un rendu découpé a pu passer entre-temps, qui ne couvre que la vue :
    // tout redessiner (créneau privé de QQuickTextEdit, joint par son nom).
    QMetaObject::invokeMethod(m_zone, "updateWholeDocument");
  }

private:
  QPointer<QQuickItem> m_zone;
};
