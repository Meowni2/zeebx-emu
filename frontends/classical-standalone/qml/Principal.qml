import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window

import zeebx

// A janela principal: a biblioteca, em grade ou no slider. O jogo abre na janela dele, como no
// egui. As configurações e os saves são das fases 5 e 6 de
// `docs/implementacao/21-migracao-para-qt.md`.
// É uma `ApplicationWindow`, e não uma `Window`, para o fundo e os textos soltos usarem a mesma
// paleta dos botões e campos: com a cor da janela tirada do sistema e a dos controles do estilo,
// o texto saía escuro sobre fundo escuro num tema escuro.
ApplicationWindow {
    id: principal

    // O jogo pedido na linha de comando, se houve: abre direto, sem passar pela lista.
    required property string jogoInicial

    width: 960
    height: 720
    title: "Zeebx"

    // Fechar a biblioteca encerra tudo, com ou sem jogo aberto — como no egui.
    onClosing: Qt.quit()

    Component.onCompleted: {
        // O modo configurado para a janela principal (`graphics.janela`), como no egui.
        const modo = biblioteca.modoDaJanela()
        if (modo === 2)
            showFullScreen()
        else if (modo === 1)
            showMaximized()
        else
            showNormal()
        if (jogoInicial !== "")
            mostra(biblioteca.abreCaminho(jogoInicial))
    }

    // `erro` vazio é jogo aberto.
    function mostra(erro) {
        aviso.text = erro
        if (erro === "")
            jogo.abre()
    }

    // O jogo escolhido agora, pela linha da lista.
    function escolhido() {
        return emSlider ? slider.linha(slider.cursor) : grade.currentIndex
    }

    // A busca ou a varredura refizeram a lista: a escolha volta ao primeiro, sem a animação do
    // slider atravessar a lista.
    function recomeca() {
        grade.currentIndex = 0
        slider.reinicia()
    }

    // O modo da biblioteca vem das configurações: 0 grade, 1 slider.
    readonly property bool emSlider: biblioteca.modoDaBiblioteca() === 1
    readonly property Item vista: emSlider ? slider : grade

    Biblioteca {
        id: biblioteca
    }

    Jogo {
        id: jogo
    }

    // O controle não gera evento no Qt, como não gerava no egui: enquanto a biblioteca está à
    // vista, ele é lido aqui. Com um jogo aberto ou a janela sem o foco, o controle não é dela —
    // e escrevendo na busca ele continua navegando, como no egui.
    Timer {
        interval: 33
        repeat: true
        running: true
        onTriggered: {
            const escutando = principal.active && !jogo.visible
            for (const codigo of biblioteca.comandos(escutando)) {
                if (codigo === 5)
                    principal.mostra(biblioteca.abreZWheel())
                else
                    principal.vista.comando(codigo)
            }
        }
    }

    // Ctrl+F leva à busca, de qualquer lugar da janela.
    Shortcut {
        sequence: StandardKey.Find
        onActivated: busca.forceActiveFocus()
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 6

        // A barra de cima.
        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Label {
                text: "Zeebx"
                font.pixelSize: 20
                font.bold: true
            }

            Button {
                text: "▶ " + biblioteca.tr("nav.z_wheel")
                enabled: biblioteca.temZWheel()
                onClicked: principal.mostra(biblioteca.abreZWheel())
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: biblioteca.tr(enabled ? "nav.z_wheel.hint" : "nav.z_wheel.missing")
            }

            // A busca: filtra a lista pelo nome, sem ligar para acentos. Esc limpa; o Enter joga
            // o escolhido, que é o primeiro resultado enquanto ninguém mexe na escolha.
            TextField {
                id: busca

                Layout.preferredWidth: 220
                placeholderText: biblioteca.tr("nav.search")
                onTextChanged: {
                    biblioteca.busca(text)
                    principal.recomeca()
                }
                Keys.onEscapePressed: {
                    text = ""
                    principal.vista.forceActiveFocus()
                }
                Keys.onReturnPressed: principal.mostra(biblioteca.abre(principal.escolhido()))
                Keys.onEnterPressed: principal.mostra(biblioteca.abre(principal.escolhido()))
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: biblioteca.tr("nav.search.hint")
            }

            ToolButton {
                text: "✕"
                visible: busca.text !== ""
                onClicked: busca.text = ""
                ToolTip.visible: hovered
                ToolTip.text: biblioteca.tr("nav.search.clear")
            }

            Item {
                Layout.fillWidth: true
            }

            // O Zeeboids só deixa sincronizar uma vez por dia, e a trava é dele: guarda a data no
            // próprio banco. Testar rede com isso custa um dia por tentativa, então o botão recua
            // a data em um dia.
            Button {
                text: biblioteca.tr("nav.unlock_sync")
                onClicked: recado.text = biblioteca.liberaSincronizacao()
                ToolTip.visible: hovered
                ToolTip.delay: 500
                ToolTip.text: biblioteca.tr("nav.unlock_sync.hint")
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: recado.text !== ""

            Label {
                id: recado

                Layout.fillWidth: true
                wrapMode: Text.Wrap
            }
            Button {
                text: biblioteca.tr("nav.unlock_sync.ok")
                onClicked: recado.text = ""
            }
        }

        // A contagem, o procurar de novo e o que apertar.
        RowLayout {
            Layout.fillWidth: true

            Label {
                text: biblioteca.contagem
            }
            Button {
                text: biblioteca.tr("library.rescan")
                onClicked: {
                    biblioteca.procuraDeNovo()
                    principal.recomeca()
                }
            }
            Label {
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
                elide: Text.ElideLeft
                opacity: 0.6
                text: biblioteca.tr("library.controls_hint")
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: palette.mid
        }

        // O que deu errado na última tentativa de abrir um jogo.
        Label {
            id: aviso

            Layout.fillWidth: true
            visible: text !== ""
            color: "#d04040"
            wrapMode: Text.Wrap
        }

        // A lista vazia diz por quê: sem pasta, pasta sem jogos, ou busca sem resultado.
        Label {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: biblioteca.vazio !== ""
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignTop
            topPadding: 48
            wrapMode: Text.Wrap
            text: biblioteca.vazio
        }

        GradeDaBiblioteca {
            id: grade

            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !principal.emSlider && biblioteca.vazio === ""
            biblioteca: biblioteca
            onAbre: (linha) => principal.mostra(biblioteca.abre(linha))
        }

        SliderDaBiblioteca {
            id: slider

            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: principal.emSlider && biblioteca.vazio === ""
            biblioteca: biblioteca
            onAbre: (linha) => principal.mostra(biblioteca.abre(linha))
        }
    }
}
