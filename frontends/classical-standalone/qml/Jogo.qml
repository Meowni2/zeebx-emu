import QtQuick
import QtQuick.Window

import zeebx

// A janela do jogo: uma janela do sistema, e não um painel dentro da principal, como no egui.
// Ela só é mostrada depois de a biblioteca abrir um jogo, e fechá-la fecha o jogo.
Window {
    id: janela

    width: 960
    height: 760
    visible: false
    color: "black"
    title: "Zeebx — " + tela.estado

    // A janela reaparece a cada jogo aberto: é a hora de ler o título dele.
    onVisibleChanged: if (visible) tela.atualiza()
    onClosing: tela.fecha()
    onActiveChanged: if (!active) tela.solta()

    TelaDoJogo {
        id: tela

        anchors.fill: parent
        anchors.bottomMargin: 24
        focus: true

        // Esc fecha e P pausa, como na janela do egui. Nenhuma das duas colide com o controle do
        // Zeebo, que usa setas, Z, X, C, V, Q, W, F, G, H, Backspace e Enter. F11 alterna a tela
        // cheia.
        Keys.onPressed: (evento) => {
            if (evento.key === Qt.Key_Escape) {
                janela.close()
            } else if (evento.key === Qt.Key_P && !evento.isAutoRepeat) {
                pausado.visible = tela.pausa()
            } else if (evento.key === Qt.Key_F11) {
                janela.visibility = janela.visibility === Window.FullScreen
                    ? Window.Windowed : Window.FullScreen
            } else if (!evento.isAutoRepeat) {
                tecla(evento.key, true)
            }
            evento.accepted = true
        }
        Keys.onReleased: (evento) => {
            if (!evento.isAutoRepeat) {
                tecla(evento.key, false)
            }
            evento.accepted = true
        }
        onActiveFocusChanged: if (!activeFocus) solta()
        // Um jogo que sai sozinho sem Z-Wheel para onde voltar fecha a janela, como no egui.
        onFechou: janela.close()
    }

    Text {
        id: pausado

        anchors.centerIn: tela
        visible: false
        color: "white"
        style: Text.Outline
        font.pixelSize: 32
        text: "pausado"
    }

    Text {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.margins: 4
        color: "#bbbbbb"
        text: tela.estado
    }

    // Um passo por quadro da janela, no ritmo do vsync — como o `request_repaint` do egui. Um
    // `Timer` de intervalo zero deixaria a CPU a 100% à toa. Só com a janela à mostra: fechada,
    // não há jogo para andar.
    FrameAnimation {
        running: janela.visible
        onTriggered: tela.passo()
    }
}
