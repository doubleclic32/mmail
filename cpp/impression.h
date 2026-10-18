// SPDX-License-Identifier: GPL-3.0-or-later
//
// Impression d'un message (0.6.3, demandée par Gwenaëlle) : le message tel
// que le volet de lecture le montre — son document de texte riche, images
// déjà chargées comprises —, précédé d'un en-tête (objet, expéditeur, date,
// pièces jointes). Sur le bureau, la boîte d'impression du système
// (QPrintDialog) ; partout, l'enregistrement en PDF (QPdfWriter).
//
// MMAIL_IMPRESSION (posé par CMakeLists.txt hors Android) : QtPrintSupport,
// qui demande une QApplication (cf. main.cpp).
#pragma once

#include <QtCore/QFileInfo>
#include <QtCore/QMarginsF>
#include <QtCore/QObject>
#include <QtCore/QStandardPaths>
#include <QtCore/QString>
#include <QtCore/QUrl>
#include <QtCore/QVariant>
#include <QtGui/QFont>
#include <QtGui/QPageLayout>
#include <QtGui/QPageSize>
#include <QtGui/QPdfWriter>
#include <QtGui/QTextBlock>
#include <QtGui/QTextCursor>
#include <QtGui/QTextDocument>
#include <QtGui/QTextFormat>
#include <QtQuick/QQuickTextDocument>

#include <memory>

#ifdef MMAIL_IMPRESSION
#include <QtPrintSupport/QPrintDialog>
#include <QtPrintSupport/QPrinter>
#endif

class Impression : public QObject
{
  Q_OBJECT
  /// Vrai si l'impression est possible ici (bureau) ; le PDF l'est partout.
  Q_PROPERTY(bool disponible READ disponible CONSTANT)

public:
  using QObject::QObject;

  bool disponible() const
  {
#ifdef MMAIL_IMPRESSION
    return true;
#else
    return false;
#endif
  }

  /// Ouvre la boîte d'impression du système, puis imprime. Rend faux si la
  /// personne a renoncé, ou si l'impression n'est pas possible ici.
  Q_INVOKABLE bool imprimer(QQuickTextDocument* corps, const QString& entete, const QString& titre)
  {
#ifdef MMAIL_IMPRESSION
    auto document = composer(corps, entete);
    if (!document)
      return false;
    QPrinter imprimante(QPrinter::HighResolution);
    imprimante.setDocName(titre);
    imprimante.setPageMargins(QMarginsF(15, 15, 15, 15), QPageLayout::Millimeter);
    QPrintDialog dialogue(&imprimante);
    dialogue.setWindowTitle(tr("Imprimer « %1 »").arg(titre));
    if (dialogue.exec() != QDialog::Accepted)
      return false;
    document->print(&imprimante);
    return true;
#else
    Q_UNUSED(corps)
    Q_UNUSED(entete)
    Q_UNUSED(titre)
    return false;
#endif
  }

  /// Dossier des documents de la personne, où le PDF est proposé.
  Q_INVOKABLE QUrl dossierDocuments() const
  {
    return QUrl::fromLocalFile(QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation));
  }

  /// Enregistre le message en PDF (A4) dans le fichier `cible` : une adresse
  /// file:// (celle de la boîte d'enregistrement) ou un chemin (essais). Un
  /// chemin Windows (C:\…) passé pour une adresse se lisait « file://C:\… »,
  /// et le PDF restait vide (essai sur le banc du 18/10).
  Q_INVOKABLE bool enregistrerPdf(QQuickTextDocument* corps, const QString& entete, const QString& titre,
                                  const QString& cible)
  {
    auto document = composer(corps, entete);
    if (!document)
      return false;
    const QUrl adresse(cible);
    const QString chemin = adresse.isLocalFile() ? adresse.toLocalFile() : cible;
    {
      QPdfWriter pdf(chemin);
      pdf.setTitle(titre);
      pdf.setCreator(QStringLiteral("MMail"));
      pdf.setResolution(300);
      pdf.setPageSize(QPageSize(QPageSize::A4));
      pdf.setPageMargins(QMarginsF(15, 15, 15, 15), QPageLayout::Millimeter);
      document->print(&pdf);
    }
    return chemin.startsWith(QStringLiteral("content:")) || QFileInfo(chemin).size() > 0;
  }

private:
  // Copie du document affiché, ses images comprises (celles que la vue a
  // chargées ne passent pas d'elles-mêmes à la copie), en-tête en tête, texte
  // à 10 points.
  static std::unique_ptr<QTextDocument> composer(QQuickTextDocument* corps, const QString& entete)
  {
    if (!corps || !corps->textDocument())
      return nullptr;
    QTextDocument* source = corps->textDocument();
    std::unique_ptr<QTextDocument> document(source->clone());
    for (QTextBlock bloc = source->begin(); bloc.isValid(); bloc = bloc.next()) {
      for (auto it = bloc.begin(); !it.atEnd(); ++it) {
        const QTextCharFormat format = it.fragment().charFormat();
        if (!format.isImageFormat())
          continue;
        const QUrl nom(format.toImageFormat().name());
        const QVariant image = source->resource(QTextDocument::ImageResource, nom);
        if (image.isValid())
          document->addResource(QTextDocument::ImageResource, nom, image);
      }
    }
    QFont police = document->defaultFont();
    police.setPointSizeF(10);
    document->setDefaultFont(police);
    QTextCursor curseur(document.get());
    curseur.movePosition(QTextCursor::Start);
    curseur.insertHtml(entete);
    curseur.insertBlock();
    curseur.insertHtml(QStringLiteral("<hr/>"));
    curseur.insertBlock();
    return document;
  }
};
