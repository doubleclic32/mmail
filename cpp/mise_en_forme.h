// SPDX-License-Identifier: GPL-3.0-or-later
//
// Mise en forme du message en cours de rédaction : ce que QML ne sait pas faire
// sur un TextArea en texte riche, et qui exige le QTextDocument qui est
// derrière — gras, italique, souligné, listes, liens —, puis ce qui part : le
// HTML et sa version en texte brut (décision 7 du dossier de projet).
#pragma once

#include <memory>

#include <QtCore/QObject>
#include <QtCore/QPointer>
#include <QtCore/QString>
#include <QtCore/QStringList>
#include <QtGui/QColor>
#include <QtGui/QFont>
#include <QtGui/QImageReader>
#include <QtGui/QTextImageFormat>
#include <QtCore/QUrl>
#include <QtGui/QTextBlock>
#include <QtGui/QTextCharFormat>
#include <QtGui/QTextCursor>
#include <QtGui/QTextDocument>
#include <QtGui/QTextList>
#include <QtQuick/QQuickTextDocument>

class MiseEnForme : public QObject
{
  Q_OBJECT
  Q_PROPERTY(QQuickTextDocument* document READ document WRITE setDocument NOTIFY documentChange)
  // Sélection du TextArea, que QML lie à ces trois propriétés.
  Q_PROPERTY(int curseur READ curseurTexte WRITE setCurseur NOTIFY etatChange)
  Q_PROPERTY(int debut READ debut WRITE setDebut NOTIFY etatChange)
  Q_PROPERTY(int fin READ fin WRITE setFin NOTIFY etatChange)
  // État sous le curseur, pour cocher les boutons de la barre.
  Q_PROPERTY(bool gras READ gras NOTIFY etatChange)
  Q_PROPERTY(bool italique READ italique NOTIFY etatChange)
  Q_PROPERTY(bool souligne READ souligne NOTIFY etatChange)
  Q_PROPERTY(bool puces READ puces NOTIFY etatChange)
  Q_PROPERTY(bool numeros READ numeros NOTIFY etatChange)

public:
  using QObject::QObject;

  QQuickTextDocument* document() const { return m_document; }
  void setDocument(QQuickTextDocument* document)
  {
    if (m_document == document)
      return;
    m_document = document;
    Q_EMIT documentChange();
    Q_EMIT etatChange();
  }
  int curseurTexte() const { return m_curseur; }
  int debut() const { return m_debut; }
  int fin() const { return m_fin; }
  void setCurseur(int v) { m_curseur = v; Q_EMIT etatChange(); }
  void setDebut(int v) { m_debut = v; Q_EMIT etatChange(); }
  void setFin(int v) { m_fin = v; Q_EMIT etatChange(); }

  bool gras() const { return format().fontWeight() >= QFont::Bold; }
  bool italique() const { return format().fontItalic(); }
  bool souligne() const { return format().fontUnderline(); }
  bool puces() const { return styleDeListe() == QTextListFormat::ListDisc; }
  bool numeros() const { return styleDeListe() == QTextListFormat::ListDecimal; }

  Q_INVOKABLE void basculerGras()
  {
    QTextCharFormat f;
    f.setFontWeight(gras() ? QFont::Normal : QFont::Bold);
    fusionner(f);
  }
  Q_INVOKABLE void basculerItalique()
  {
    QTextCharFormat f;
    f.setFontItalic(!italique());
    fusionner(f);
  }
  Q_INVOKABLE void basculerSouligne()
  {
    QTextCharFormat f;
    f.setFontUnderline(!souligne());
    fusionner(f);
  }

  /// Met les paragraphes sélectionnés en liste à puces (`numerotee` faux) ou
  /// numérotée, ou les en retire s'ils y sont déjà.
  Q_INVOKABLE void basculerListe(bool numerotee)
  {
    QTextDocument* doc = texte_();
    if (!doc)
      return;
    const auto style = numerotee ? QTextListFormat::ListDecimal : QTextListFormat::ListDisc;
    QTextCursor c = curseur();
    c.beginEditBlock();
    if (styleDeListe() == style) {
      // Retirer : chaque bloc sélectionné sort de sa liste, sans retrait.
      const QTextBlock dernier = doc->findBlock(borneFin());
      for (QTextBlock b = doc->findBlock(borneDebut()); b.isValid(); b = b.next()) {
        if (QTextList* l = b.textList())
          l->remove(b);
        QTextCursor cb(b);
        QTextBlockFormat bf = b.blockFormat();
        bf.setIndent(0);
        cb.setBlockFormat(bf);
        if (b == dernier)
          break;
      }
    } else {
      QTextListFormat lf;
      lf.setStyle(style);
      lf.setIndent(1);
      c.createList(lf);
    }
    c.endEditBlock();
    Q_EMIT etatChange();
  }

  /// Fait de la sélection un lien vers `url`. Sans sélection, insère l'adresse
  /// elle-même, en lien.
  Q_INVOKABLE void poserLien(const QString& url)
  {
    const QString cible = url.trimmed();
    if (cible.isEmpty() || !texte_())
      return;
    QTextCharFormat f;
    f.setAnchor(true);
    f.setAnchorHref(cible);
    f.setFontUnderline(true);
    f.setForeground(QColor(0x1a, 0x44, 0x80));
    QTextCursor c = curseur();
    if (c.hasSelection())
      c.mergeCharFormat(f);
    else
      c.insertText(cible, f);
    Q_EMIT etatChange();
  }

  /// Retire gras, italique, soulignement et liens de la sélection.
  Q_INVOKABLE void effacerMiseEnForme()
  {
    QTextCursor c = curseur();
    if (!c.hasSelection())
      c.select(QTextCursor::WordUnderCursor);
    c.setCharFormat(QTextCharFormat());
    Q_EMIT etatChange();
  }

  /// Remplace tout le contenu par du texte brut, ou par du HTML.
  Q_INVOKABLE void poserTexte(const QString& texte)
  {
    if (QTextDocument* doc = texte_())
      doc->setPlainText(texte);
  }
  Q_INVOKABLE void poserHtml(const QString& html)
  {
    if (QTextDocument* doc = texte_())
      doc->setHtml(html);
  }

  /// Insère du HTML (une signature avec ses images) à la position donnée.
  Q_INVOKABLE void insererHtml(int position, const QString& html)
  {
    QTextDocument* doc = texte_();
    if (!doc)
      return;
    QTextCursor c(doc);
    c.setPosition(qBound(0, position, doc->characterCount() - 1));
    c.insertHtml(html);
    Q_EMIT etatChange();
  }

  /// Insère une image au curseur, désignée par son adresse `file:`. Une image
  /// plus large que `largeurMax` pixels y est ramenée, proportions gardées.
  Q_INVOKABLE void insererImage(const QString& url, int largeurMax)
  {
    QTextDocument* doc = texte_();
    if (!doc || url.isEmpty())
      return;
    QTextImageFormat f;
    f.setName(url);
    const QSize taille = QImageReader(QUrl(url).toLocalFile()).size();
    if (taille.isValid() && largeurMax > 0 && taille.width() > largeurMax) {
      f.setWidth(largeurMax);
      f.setHeight(qRound(double(taille.height()) * largeurMax / taille.width()));
    }
    curseur().insertImage(f);
    Q_EMIT etatChange();
  }

  /// Ramène à `largeurMax` pixels, proportions gardées, toute image plus
  /// large ; une image déjà ramenée retrouve la taille voulue par le message
  /// quand la place s'élargit. Pour la lecture : la page suit la largeur de
  /// la colonne, une image qui la dépasserait serait rognée.
  Q_INVOKABLE void bornerImages(int largeurMax)
  {
    QTextDocument* doc = texte_();
    if (!doc || largeurMax <= 0)
      return;
    // Même document, même largeur : rien à refaire. La minuterie de la colonne
    // et le zoom l'appellent souvent pour rien.
    if (doc == m_documentBorne && doc->revision() == m_revisionBornee && largeurMax == m_largeurBornee)
      return;
    // Positions d'abord : changer un format redécoupe les fragments.
    QList<int> positions;
    for (QTextBlock b = doc->begin(); b.isValid(); b = b.next())
      for (auto it = b.begin(); !it.atEnd(); ++it)
        if (it.fragment().charFormat().isImageFormat())
          for (int i = 0; i < it.fragment().length(); ++i)
            positions.append(it.fragment().position() + i);

    QTextCursor c(doc);
    c.beginEditBlock();
    for (int position : positions) {
      c.setPosition(position);
      c.setPosition(position + 1, QTextCursor::KeepAnchor);
      QTextImageFormat f = c.charFormat().toImageFormat();
      if (!f.isValid())
        continue;
      // Taille écrite dans le message, retenue à la première passe ; 0 : non
      // précisée.
      if (!f.hasProperty(LargeurVoulue)) {
        f.setProperty(LargeurVoulue, f.hasProperty(QTextFormat::ImageWidth) ? f.width() : 0.0);
        f.setProperty(HauteurVoulue, f.hasProperty(QTextFormat::ImageHeight) ? f.height() : 0.0);
      }
      const qreal largeur = f.property(LargeurVoulue).toReal();
      const qreal hauteur = f.property(HauteurVoulue).toReal();
      // Taille du fichier, lue une fois et gardée sur le format : relire
      // chaque image du disque à chaque largeur coûtait plus que le reste.
      if (!f.hasProperty(LargeurNaturelle)) {
        const QSize lue = QImageReader(QUrl(f.name()).toLocalFile()).size();
        f.setProperty(LargeurNaturelle, lue.width());
        f.setProperty(HauteurNaturelle, lue.height());
      }
      const QSize naturelle(f.property(LargeurNaturelle).toInt(), f.property(HauteurNaturelle).toInt());
      // Taille affichée sans contrainte : celle du message, l'autre côté
      // suivant les proportions de l'image.
      qreal l = largeur, h = hauteur;
      if (naturelle.isValid() && naturelle.width() > 0 && naturelle.height() > 0) {
        if (l <= 0 && h <= 0) {
          l = naturelle.width();
          h = naturelle.height();
        } else if (l <= 0) {
          l = h * naturelle.width() / naturelle.height();
        } else if (h <= 0) {
          h = l * naturelle.height() / naturelle.width();
        }
      }
      if (l > largeurMax && h > 0) {
        f.setWidth(largeurMax);
        f.setHeight(h * largeurMax / l);
      } else {
        if (largeur > 0)
          f.setWidth(largeur);
        else
          f.clearProperty(QTextFormat::ImageWidth);
        if (hauteur > 0)
          f.setHeight(hauteur);
        else
          f.clearProperty(QTextFormat::ImageHeight);
      }
      if (f != c.charFormat())
        c.setCharFormat(f);
    }
    c.endEditBlock();
    m_documentBorne = doc;
    m_revisionBornee = doc->revision();
    m_largeurBornee = largeurMax;
  }

  /// Insère du texte brut (une signature) à la position donnée.
  Q_INVOKABLE void inserer(int position, const QString& texte)
  {
    QTextDocument* doc = texte_();
    if (!doc)
      return;
    QTextCursor c(doc);
    c.setPosition(qBound(0, position, doc->characterCount() - 1));
    c.insertText(texte);
  }

  /// Remplace le caractère à la position donnée, mise en forme gardée.
  Q_INVOKABLE void remplacerCaractere(int position, const QString& texte)
  {
    QTextDocument* doc = texte_();
    if (!doc || position < 0 || position >= doc->characterCount() - 1)
      return;
    QTextCursor c(doc);
    c.setPosition(position);
    c.setPosition(position + 1, QTextCursor::KeepAnchor);
    c.insertText(texte, c.charFormat());
  }

  /// La position ouvre-t-elle une phrase ? Début de paragraphe ou de ligne,
  /// ou une espace après « . », « ! » ou « ? » — guillemet ou parenthèse
  /// fermants admis entre les deux —, sauf après des points de suspension,
  /// une initiale, une abréviation courante (« etc. », « cf. »…) ou un
  /// numéro en tête de ligne (« 1. »). Majuscule automatique de la frappe
  /// (0.6.12, demande de Fabienne).
  Q_INVOKABLE bool debutDePhrase(int position) const
  {
    QTextDocument* doc = texte_();
    if (!doc)
      return false;
    const QTextBlock bloc = doc->findBlock(position);
    if (!bloc.isValid())
      return false;
    QString avant = bloc.text().left(position - bloc.position());
    // Maj+Entrée : nouvelle ligne dans le paragraphe.
    avant = avant.mid(avant.lastIndexOf(QChar::LineSeparator) + 1);
    int i = avant.size();
    while (i > 0 && avant.at(i - 1).isSpace())
      --i;
    if (i == 0)
      return true;
    if (i == avant.size())
      return false;
    while (i > 0 && QStringLiteral(")]»\"'’").contains(avant.at(i - 1)))
      --i;
    if (i == 0)
      return false;
    const QChar ponctuation = avant.at(i - 1);
    if (ponctuation == u'!' || ponctuation == u'?')
      return true;
    if (ponctuation != u'.' || (i >= 2 && avant.at(i - 2) == u'.'))
      return false;
    int debutMot = i - 1;
    while (debutMot > 0 && avant.at(debutMot - 1).isLetterOrNumber())
      --debutMot;
    const QString mot = avant.mid(debutMot, i - 1 - debutMot).toLower();
    if (mot.isEmpty())
      return false;
    bool nombre = false;
    mot.toLongLong(&nombre);
    if (nombre)
      return !avant.left(debutMot).trimmed().isEmpty();
    static const QStringList abreviations = {
      QStringLiteral("etc"), QStringLiteral("cf"), QStringLiteral("ex"), QStringLiteral("env"),
      QStringLiteral("réf"), QStringLiteral("ref"), QStringLiteral("tél"), QStringLiteral("tel"),
      QStringLiteral("art"), QStringLiteral("av"), QStringLiteral("bd"), QStringLiteral("pp"),
      QStringLiteral("vol"), QStringLiteral("chap"), QStringLiteral("fig"), QStringLiteral("min"),
      QStringLiteral("max"), QStringLiteral("hab"), QStringLiteral("dépt"), QStringLiteral("éd")
    };
    return mot.size() > 1 && !abreviations.contains(mot);
  }

  /// HTML à envoyer. La police d'affichage — celle de l'utilisateur, zoom
  /// compris — ne part pas : le corps est rendu dans une police courante, à
  /// taille normale, chez le destinataire.
  Q_INVOKABLE QString html() const
  {
    QTextDocument* doc = texte_();
    if (!doc)
      return QString();
    std::unique_ptr<QTextDocument> copie(doc->clone());
    QFont police(QStringLiteral("Arial"));
    police.setPointSizeF(11);
    copie->setDefaultFont(police);
    return copie->toHtml();
  }

  /// Version en texte brut : chaque paragraphe sur sa ligne, les listes avec
  /// leurs puces ou leurs numéros, les liens suivis de leur adresse.
  Q_INVOKABLE QString texte() const
  {
    QTextDocument* doc = texte_();
    if (!doc)
      return QString();
    QStringList lignes;
    for (QTextBlock b = doc->begin(); b.isValid(); b = b.next()) {
      QString ligne;
      for (auto it = b.begin(); !it.atEnd(); ++it) {
        const QTextFragment fragment = it.fragment();
        if (!fragment.isValid())
          continue;
        ligne += fragment.text();
        const QString href = fragment.charFormat().anchorHref();
        if (!href.isEmpty() && href != fragment.text())
          ligne += QStringLiteral(" <") + href + QLatin1Char('>');
      }
      if (QTextList* l = b.textList()) {
        const bool numerotee = l->format().style() == QTextListFormat::ListDecimal;
        ligne = (numerotee ? QString::number(l->itemNumber(b) + 1) + QStringLiteral(". ")
                           : QStringLiteral("• "))
                + ligne;
      }
      lignes << ligne;
    }
    QString sortie = lignes.join(QLatin1Char('\n'));
    sortie.replace(QChar::LineSeparator, QLatin1Char('\n'));
    sortie.replace(QChar::Nbsp, QLatin1Char(' '));
    // Une image tient la place d'un caractère d'objet : rien dans le texte brut.
    sortie.remove(QChar::ObjectReplacementCharacter);
    return sortie;
  }

  /// Vrai si le message porte une mise en forme que le texte brut perdrait.
  Q_INVOKABLE bool enrichi() const
  {
    QTextDocument* doc = texte_();
    if (!doc)
      return false;
    for (QTextBlock b = doc->begin(); b.isValid(); b = b.next()) {
      if (b.textList())
        return true;
      for (auto it = b.begin(); !it.atEnd(); ++it) {
        const QTextCharFormat f = it.fragment().charFormat();
        if (f.fontWeight() >= QFont::Bold || f.fontItalic() || f.fontUnderline() || f.isAnchor()
            || f.isImageFormat())
          return true;
      }
    }
    return false;
  }

Q_SIGNALS:
  void documentChange();
  void etatChange();

private:
  /// Taille d'une image telle que le message l'écrit, gardée sur son format
  /// pour que `bornerImages` puisse la rendre.
  enum { LargeurVoulue = QTextFormat::UserProperty + 1, HauteurVoulue, LargeurNaturelle, HauteurNaturelle };

  /// Dernier passage de `bornerImages` : document, révision, largeur.
  QTextDocument* m_documentBorne = nullptr;
  int m_revisionBornee = -1;
  int m_largeurBornee = -1;

  QTextDocument* texte_() const { return m_document ? m_document->textDocument() : nullptr; }

  /// Bornes de la sélection, ou le curseur deux fois s'il n'y en a pas.
  int borneDebut() const { return m_debut != m_fin ? qMin(m_debut, m_fin) : m_curseur; }
  int borneFin() const { return m_debut != m_fin ? qMax(m_debut, m_fin) : m_curseur; }

  QTextCursor curseur() const
  {
    QTextDocument* doc = texte_();
    QTextCursor c(doc);
    if (!doc)
      return c;
    const int dernier = doc->characterCount() - 1;
    if (m_debut != m_fin) {
      c.setPosition(qBound(0, m_debut, dernier));
      c.setPosition(qBound(0, m_fin, dernier), QTextCursor::KeepAnchor);
    } else {
      c.setPosition(qBound(0, m_curseur, dernier));
    }
    return c;
  }

  /// Format du caractère sous le curseur, ou du début de la sélection.
  QTextCharFormat format() const
  {
    QTextDocument* doc = texte_();
    if (!doc)
      return QTextCharFormat();
    QTextCursor c(doc);
    const int dernier = doc->characterCount() - 1;
    // Le format « au curseur » est celui du caractère qui précède : c'est ce
    // que prolonge la frappe.
    const int position = m_debut != m_fin ? qMin(m_debut, m_fin) + 1 : m_curseur;
    c.setPosition(qBound(0, position, dernier));
    return c.charFormat();
  }

  QTextListFormat::Style styleDeListe() const
  {
    QTextDocument* doc = texte_();
    if (!doc)
      return QTextListFormat::ListStyleUndefined;
    QTextList* l = doc->findBlock(borneDebut()).textList();
    return l ? l->format().style() : QTextListFormat::ListStyleUndefined;
  }

  /// Applique un format à la sélection ; sans sélection, au mot sous le
  /// curseur, comme le fait un traitement de texte.
  void fusionner(const QTextCharFormat& f)
  {
    QTextCursor c = curseur();
    if (!c.hasSelection())
      c.select(QTextCursor::WordUnderCursor);
    c.mergeCharFormat(f);
    Q_EMIT etatChange();
  }

  QPointer<QQuickTextDocument> m_document;
  int m_curseur = 0;
  int m_debut = 0;
  int m_fin = 0;
};
