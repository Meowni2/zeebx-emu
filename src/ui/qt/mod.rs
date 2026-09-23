//! A interface em Qt Quick, em prova de viabilidade. Ver
//! `docs/implementacao/21-migracao-para-qt.md`.
//!
//! `zeebx qt <jogo> [--placa]` abre uma janela QML com o jogo. Não substitui nada ainda: a
//! interface continua sendo a do egui.

pub mod ponte;

use std::process::ExitCode;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl, QVariant};

pub fn launch(jogo: &str, placa: bool) -> ExitCode {
    // Antes do `QGuiApplication`: depois dele a API gráfica e o formato já estão escolhidos.
    ponte::qobject::prepara_gl();
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    let (Some(app), Some(mut engine)) = (app.as_mut(), engine.as_mut()) else {
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
    match app.exec() {
        0 => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}
