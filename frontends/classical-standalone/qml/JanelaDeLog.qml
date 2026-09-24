import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

import zeebx

// A janela de log da execução. Separada da do jogo de propósito, como no egui: quem está lendo
// log quer as duas coisas ao mesmo tempo, e um painel dentro da janela do jogo roubaria espaço do
// quadro. Ela acompanha o jogo: só existe enquanto há execução para registrar.
ApplicationWindow {
    id: janela

    // Devolve `valor`, e faz a ligação que chama isto depender das `versoes`.
    //
    // **As versões vão como argumento, e não numa expressão solta.** O QML é compilado
    // antecipadamente (qmlcachegen), e o compilador descarta uma leitura cujo valor não é usado:
    // `(cfg.versao, cfg.aparelho())` perdia a leitura da versão, a ligação deixava de depender
    // dela, e trocar o aparelho de Boomerang para Z-Pad não trocava a tela. Interpretado, como no
    // qmltestrunner, funcionava — por isso os testes não pegaram.
    function depende(versoes, valor) {
        return valor
    }

    // Se a janela deve estar à mostra: o jogo aberto, a opção ligada, e não fechada nesta
    // execução. Quem liga é a principal, que sabe do jogo e das configurações.
    property bool pedida: false
    // Quantas vezes a janela foi fechada, para a ligação de `pedida` reler o `Log`.
    property int dispensas: 0
    // O que deu a última exportação.
    property string recado: ""
    readonly property alias log: log

    width: 640
    height: 420
    minimumWidth: 360
    minimumHeight: 200
    visible: false
    title: "Zeebx — " + tr("debug.log.title")

    function tr(chave) {
        return depende([Idioma.versao], Idioma.texto(chave))
    }

    onPedidaChanged: {
        if (pedida) {
            recado = ""
            presa = true
            releLinhas()
            // Junto com a janela do jogo, e não depois: o jogo pede o foco por último, e fica
            // com o teclado. Mostrado depois, o log ficava com o foco — o KDE no Wayland recusa
            // devolvê-lo ao jogo. Por cima do jogo ele fica por ser filho dele.
            showNormal()
        } else {
            hide()
        }
    }

    // Fechar a janela dispensa o log **desta** execução, e não a preferência: gravar isso
    // desligaria a opção sozinha, e a janela abriria uma vez e nunca mais.
    onClosing: {
        log.dispensa()
        dispensas += 1
    }

    Log {
        id: log
    }

    // As linhas mudam enquanto o jogo roda, e o log não avisa: é relido de tempos em tempos.
    // Relido só se mudou, para não perder a rolagem de quem está lendo o meio.
    property var linhas: []
    // Preso no fim: log que não acompanha o que acabou de acontecer não serve. Quem sobe para
    // ler solta a lista, e quem desce até o fim prende de novo. Só o usuário muda isto: a troca
    // das linhas mexe na rolagem, e é marcada como `ajustando` para não contar.
    property bool presa: true
    property bool ajustando: false
    function releLinhas() {
        const novas = log.linhas()
        if (novas.length === linhas.length
                && (novas.length === 0 || novas[novas.length - 1] === linhas[linhas.length - 1]))
            return
        const altura = lista.contentY
        ajustando = true
        linhas = novas
        if (presa)
            lista.positionViewAtEnd()
        else
            lista.contentY = altura
        ajustando = false
        // Na primeira leitura a janela ainda nem apareceu, e a lista não tem altura: o fim só
        // existe depois do layout.
        if (presa)
            Qt.callLater(janela.vaiAoFim)
    }
    function vaiAoFim() {
        ajustando = true
        lista.positionViewAtEnd()
        ajustando = false
    }
    Timer {
        interval: 250
        repeat: true
        running: janela.visible
        onTriggered: janela.releLinhas()
    }

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8

            Button {
                text: janela.tr("debug.log.export")
                onClicked: {
                    const recado = log.exporta()
                    if (recado !== "")
                        janela.recado = recado
                }
            }
            // O que deu a exportação, ou, antes dela, o caminho fixo à vista: quem quer o arquivo
            // não precisa exportar nada.
            Label {
                Layout.fillWidth: true
                elide: Text.ElideMiddle
                opacity: 0.6
                text: janela.recado !== "" ? janela.recado : janela.pedida ? log.caminho() : ""
            }
        }
    }

    Label {
        anchors.centerIn: parent
        visible: janela.linhas.length === 0
        opacity: 0.6
        text: janela.tr("debug.log.empty")
    }

    ListView {
        id: lista

        anchors.fill: parent
        anchors.margins: 8
        clip: true
        model: janela.linhas
        boundsBehavior: Flickable.StopAtBounds
        onContentYChanged: if (!janela.ajustando) janela.presa = atYEnd
        ScrollBar.vertical: ScrollBar {}

        delegate: Text {
            required property string modelData

            width: ListView.view.width
            wrapMode: Text.WrapAnywhere
            color: palette.text
            font.family: "monospace"
            text: modelData
        }
    }
}
