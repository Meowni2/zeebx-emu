#include "gl_qt.h"

#include <QtCore/QCoreApplication>
#include <QtGui/QOffscreenSurface>
#include <QtGui/QOpenGLContext>
#include <QtGui/QSurfaceFormat>
#include <QtQuick/QQuickWindow>
#include <QtQuick/QSGRendererInterface>

#include <memory>

namespace zeebx {

namespace {
// Vivem até o fim do processo: o contexto não pode morrer com a janela do jogo, porque a
// Z-Wheel reabre jogos e as texturas do rasterizador moram nele.
std::unique_ptr<QOpenGLContext> contexto;
std::unique_ptr<QOffscreenSurface> superficie;
} // namespace

void prepara_gl()
{
    // O `threaded`, padrão no Linux com Mesa, desenharia o scene graph noutra thread enquanto a
    // principal escreve o quadro seguinte. Quem define a variável à mão continua mandando.
    if (!qEnvironmentVariableIsSet("QSG_RENDER_LOOP")) {
        qputenv("QSG_RENDER_LOOP", "basic");
    }
    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
    // No macOS o padrão é Metal e no Windows é D3D; o rasterizador é GL.
    QQuickWindow::setGraphicsApi(QSGRendererInterface::OpenGL);
    // O contrato que o eframe entregava, e que os shaders do `Pintor` e do `GpuState` esperam.
    QSurfaceFormat formato;
    // GL de desktop, dito explicitamente. Sem isto, no Wayland com o driver da NVIDIA, o pedido
    // de 3.3 core volta EGL_BAD_MATCH e o Qt Quick aborta ao abrir a janela; no X11 passava.
    formato.setRenderableType(QSurfaceFormat::OpenGL);
    formato.setVersion(3, 3);
    formato.setProfile(QSurfaceFormat::CoreProfile);
    formato.setDepthBufferSize(24);
    formato.setStencilBufferSize(8);
    QSurfaceFormat::setDefaultFormat(formato);
}

bool gl_cria()
{
    if (contexto) {
        return true;
    }
    auto novo = std::make_unique<QOpenGLContext>();
    novo->setFormat(QSurfaceFormat::defaultFormat());
    novo->setShareContext(QOpenGLContext::globalShareContext());
    if (!novo->create()) {
        return false;
    }
    auto fora_de_tela = std::make_unique<QOffscreenSurface>();
    fora_de_tela->setFormat(novo->format());
    fora_de_tela->create();
    if (!fora_de_tela->isValid()) {
        return false;
    }
    contexto = std::move(novo);
    superficie = std::move(fora_de_tela);
    return true;
}

bool gl_torna_corrente()
{
    return contexto && contexto->makeCurrent(superficie.get());
}

void gl_solta()
{
    if (contexto) {
        contexto->doneCurrent();
    }
}

std::size_t gl_funcao(rust::Str nome)
{
    if (!contexto) {
        return 0;
    }
    return reinterpret_cast<std::size_t>(contexto->getProcAddress(QByteArray(nome.data(), qsizetype(nome.size()))));
}

} // namespace zeebx
