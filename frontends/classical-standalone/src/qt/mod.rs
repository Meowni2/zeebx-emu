//! A interface em Qt Quick, em prova de viabilidade. Ver
//! `docs/implementacao/21-migracao-para-qt.md`.
//!
//! `zeebx qt <jogo> [--placa]` abre uma janela QML com o jogo. Não substitui nada ainda: a
//! interface continua sendo a do egui.

mod ponte;

use std::process::ExitCode;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl, QVariant};

pub fn launch(jogo: &str, placa: bool) -> ExitCode {
    // Antes do `QGuiApplication`: depois dele a API gráfica e o formato já estão escolhidos.
    ponte::qobject::prepara_gl();
    let mut app_dono = QGuiApplication::new();
    let mut engine_dono = QQmlApplicationEngine::new();
    let (Some(app), Some(mut engine)) = (app_dono.as_mut(), engine_dono.as_mut()) else {
        eprintln!("erro: o Qt não abriu");
        return ExitCode::FAILURE;
    };
    let caminho = std::fs::canonicalize(jogo)
        .map(|c| c.display().to_string())
        .unwrap_or_else(|_| jogo.to_string());
    let mut propriedades = cxx_qt_lib::QMap::<cxx_qt_lib::QMapPair_QString_QVariant>::default();
    propriedades.insert(QString::from("jogo"), QVariant::from(&QString::from(&caminho)));
    propriedades.insert(QString::from("placa"), QVariant::from(&placa));
    engine.as_mut().set_initial_properties(&propriedades);
    engine.load(&QUrl::from("qrc:/qt/qml/zeebx/qml/Principal.qml"));
    let saida = app.exec();
    // **A ordem é a correção.** A engine leva a tela, e a tela solta a sessão com o contexto de
    // GL ainda vivo; depois o contexto; e só então o `QGuiApplication`. Deixado para o fim do
    // processo, o contexto fora de tela morria depois do Qt, e fechar a janela terminava em
    // SIGSEGV dentro do `~QOffscreenSurface`.
    drop(engine_dono);
    ponte::qobject::gl_destroi();
    drop(app_dono);
    match saida {
        0 => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}
