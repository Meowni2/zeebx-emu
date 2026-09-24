#include "quadro.h"

#include <QtQuick/QQuickWindow>
#include <QtQuick/QSGSimpleTextureNode>
#include <QtQuick/QSGTexture>

ItemDoQuadro::ItemDoQuadro(QQuickItem *parent)
    : QQuickItem(parent)
{
    setFlag(ItemHasContents, true);
}

void ItemDoQuadro::mostraTextura(quint32 textura, float recorteU, float recorteV, QRectF destino,
                                 bool suave)
{
    m_fonte = Fonte::Textura;
    m_textura = textura;
    m_recorte = QRectF(0, 0, recorteU, recorteV);
    m_destino = destino;
    m_suave = suave;
    m_imagem = QImage();
    update();
}

void ItemDoQuadro::mostraImagem(const QImage &imagem, QRectF destino, bool suave)
{
    m_fonte = Fonte::Imagem;
    m_imagem = imagem;
    m_imagemNova = true;
    m_destino = destino;
    m_suave = suave;
    update();
}

void ItemDoQuadro::posiciona(QRectF destino, bool suave)
{
    if (destino != m_destino || suave != m_suave) {
        m_destino = destino;
        m_suave = suave;
        update();
    }
}

void ItemDoQuadro::esvazia()
{
    m_fonte = Fonte::Nada;
    m_textura = 0;
    m_imagem = QImage();
    update();
}

QSGNode *ItemDoQuadro::updatePaintNode(QSGNode *antigo, UpdatePaintNodeData *)
{
    auto *no = static_cast<QSGSimpleTextureNode *>(antigo);
    if (m_fonte == Fonte::Nada || window() == nullptr) {
        m_embrulhada = 0;
        delete no;
        return nullptr;
    }
    if (no == nullptr) {
        no = new QSGSimpleTextureNode;
        no->setOwnsTexture(true);
    }

    if (m_fonte == Fonte::Textura) {
        // A textura é do rasterizador: o embrulho não a possui, e é refeito só quando o nome muda.
        //
        // **O tamanho declarado é 1×1 de propósito.** O Qt usa o tamanho da textura só para
        // converter o `sourceRect` em coordenadas de 0 a 1, e o rasterizador já entrega o recorte
        // nessas coordenadas. O tamanho real muda com a resolução interna e as colunas do 16:9, e
        // o GL 3.3 do núcleo não o pergunta ao driver.
        if (no->texture() == nullptr || m_embrulhada != m_textura) {
            auto *textura = QNativeInterface::QSGOpenGLTexture::fromNative(
                m_textura, window(), QSize(1, 1), QQuickWindow::TextureIsOpaque);
            no->setTexture(textura);
            m_embrulhada = m_textura;
        }
        no->setSourceRect(m_recorte);
    } else if (m_imagemNova || no->texture() == nullptr || m_embrulhada != 0) {
        // O 2D: a imagem só sobe quando mudou, a mesma regra do egui.
        no->setTexture(window()->createTextureFromImage(m_imagem));
        m_embrulhada = 0;
        no->setSourceRect(QRectF(QPointF(0, 0), m_imagem.size()));
        m_imagemNova = false;
    }

    no->setRect(m_destino);
    no->setFiltering(m_suave ? QSGTexture::Linear : QSGTexture::Nearest);
    return no;
}
