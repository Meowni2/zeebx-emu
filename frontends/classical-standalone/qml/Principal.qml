import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window

import zeebx

// A janela principal: a biblioteca. O jogo abre na janela dele, como no egui.
//
// Por enquanto é uma lista de títulos. A grade, o slider, as capas e a busca são da fase 4 de
// `docs/implementacao/21-migracao-para-qt.md`.
Window {
    id: principal

    // O jogo pedido na linha de comando, se houve: abre direto, sem passar pela lista.
    required property string jogoInicial

    width: 960
    height: 720
    visible: true
    title: "Zeebx"

    // Fechar a biblioteca encerra tudo, com ou sem jogo aberto — como no egui.
    onClosing: Qt.quit()

    Component.onCompleted: {
        if (jogoInicial !== "")
            mostra(biblioteca.abreCaminho(jogoInicial))
    }

    // `erro` vazio é jogo aberto.
    function mostra(erro) {
        aviso.text = erro
        if (erro === "") {
            jogo.show()
            jogo.raise()
            jogo.requestActivate()
        }
    }

    Biblioteca {
        id: biblioteca
    }

    Jogo {
        id: jogo
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 8

        RowLayout {
            Layout.fillWidth: true

            Button {
                text: "▶ Z-Wheel"
                enabled: biblioteca.temZWheel()
                onClicked: principal.mostra(biblioteca.abreZWheel())
            }

            Label {
                Layout.fillWidth: true
                elide: Text.ElideMiddle
                text: lista.count + " jogos em " + biblioteca.pasta()
            }
        }

        Label {
            id: aviso

            Layout.fillWidth: true
            visible: text !== ""
            color: "#d04040"
            wrapMode: Text.Wrap
        }

        ListView {
            id: lista

            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            focus: true
            model: biblioteca
            ScrollBar.vertical: ScrollBar {}

            delegate: ItemDelegate {
                required property int index
                required property string titulo

                width: ListView.view.width
                text: titulo
                highlighted: ListView.isCurrentItem
                onClicked: principal.mostra(biblioteca.abre(index))
            }

            Keys.onReturnPressed: principal.mostra(biblioteca.abre(currentIndex))
        }
    }
}
