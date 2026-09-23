import QtQuick
import QtQuick.Window

import zeebx

// A janela do jogo da prova de viabilidade: a tela, a linha de estado e o passo a cada quadro.
Window {
    id: janela

    required property string jogo
    required property bool placa

    width: 960
    height: 760
    visible: true
    color: "black"
    title: "Zeebx (Qt) — " + tela.estado

    TelaDoJogo {
        id: tela

        anchors.fill: parent
        anchors.bottomMargin: 24
        focus: true

        Component.onCompleted: abre(janela.jogo, janela.placa)

        Keys.onPressed: (evento) => {
            if (evento.key === Qt.Key_F11) {
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
        // Um jogo aberto aqui que sai sozinho — sem Z-Wheel para onde voltar — fecha a janela,
        // como a do egui.
        onFechou: janela.close()
    }

    Text {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.margins: 4
        color: "#bbbbbb"
        text: tela.estado
    }

    // Um passo por quadro da janela, no ritmo do vsync — como o `request_repaint` do egui. Um
    // `Timer` de intervalo zero deixaria a CPU a 100% à toa.
    FrameAnimation {
        running: true
        onTriggered: tela.passo()
    }

    onActiveChanged: if (!active) tela.solta()
}
