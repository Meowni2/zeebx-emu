//! A interface em Qt Quick, em migração. Ver `docs/implementacao/21-migracao-para-qt.md`.
//!
//! `zeebx qt` abre a biblioteca; `zeebx qt <jogo>` abre direto o jogo. Não substitui nada ainda:
//! a interface padrão continua sendo a do egui.

mod biblioteca;
mod nucleo;
mod ponte;

use std::process::ExitCode;

use cxx_qt::casting::Upcast;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl, QVariant};

pub fn launch(jogo: Option<&str>) -> ExitCode {
    // Antes do `QGuiApplication`: depois dele a API gráfica e o formato já estão escolhidos.
    ponte::qobject::prepara_gl();
    let mut app_dono = QGuiApplication::new();
    let mut engine_dono = QQmlApplicationEngine::new();
    let (Some(app), Some(mut engine)) = (app_dono.as_mut(), engine_dono.as_mut()) else {
        eprintln!("erro: o Qt não abriu");
        return ExitCode::FAILURE;
    };
    let mut propriedades = cxx_qt_lib::QMap::<cxx_qt_lib::QMapPair_QString_QVariant>::default();
    let jogo = QString::from(jogo.unwrap_or_default());
    propriedades.insert(QString::from("jogoInicial"), QVariant::from(&jogo));
    engine.as_mut().set_initial_properties(&propriedades);
    // As capas, os logos e as classificações da biblioteca: `image://zeebx/…`.
    biblioteca::registra_imagens(engine.as_mut().upcast_pin());
    engine.load(&QUrl::from("qrc:/qt/qml/zeebx/qml/Principal.qml"));
    let saida = app.exec();
    // **A ordem é a correção.** Primeiro a engine; depois o jogo, solto com o contexto de GL
    // ainda vivo, porque as texturas dele moram lá; depois o contexto; e só então o
    // `QGuiApplication`. Deixado para o fim do processo, o contexto fora de tela morria depois do
    // Qt, e fechar a janela terminava em SIGSEGV dentro do `~QOffscreenSurface`.
    drop(engine_dono);
    nucleo::encerra();
    ponte::qobject::gl_destroi();
    drop(app_dono);
    match saida {
        0 => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}
