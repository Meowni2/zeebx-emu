fn main() {
    // O unicorn-engine-sys traz o cputlb do QEMU, que usa atômicos de 128 bits
    // (__atomic_load_16, __atomic_store_16, __atomic_compare_exchange_16). No
    // x86-64 o compilador não os gera inline sem cmpxchg16b, então eles vivem
    // na libatomic do GCC e precisam ser linkados explicitamente.
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=atomic");
    }
    // O ícone e a versão entram no `.exe`. O `CARGO_CFG_TARGET_OS` é o do alvo, e não o da
    // máquina que compila: o `cfg!` aqui responderia pelo sistema em que o build.rs roda.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut recurso = winresource::WindowsResource::new();
        recurso.set_icon("assets/icones/zeebx.ico");
        if let Err(erro) = recurso.compile() {
            println!("cargo:warning=sem ícone no .exe: {erro}");
        }
    }
    #[cfg(feature = "ui-qt")]
    interface_qt();
    println!("cargo:rerun-if-changed=assets/icones/zeebx.ico");
    println!("cargo:rerun-if-changed=build.rs");
}

/// A ponte com o Qt: os `QObject`s escritos em Rust, a cola em C++ e o módulo QML. Ver
/// `docs/implementacao/21-migracao-para-qt.md`.
#[cfg(feature = "ui-qt")]
fn interface_qt() {
    use cxx_qt_build::{CxxQtBuilder, QmlModule};

    CxxQtBuilder::new_qml_module(
        QmlModule::new("zeebx").qml_file("qml/Principal.qml").depend("QtQuick"),
    )
    .files(["src/ui/qt/ponte.rs"])
    .include_dir("src/ui/qt/cpp")
    .cpp_files(["src/ui/qt/cpp/gl_qt.cpp"])
    .qt_module("Quick")
    // O Qt Qml pede o Network no macOS.
    .qt_module("Network")
    .build();
    println!("cargo:rerun-if-changed=src/ui/qt/cpp/gl_qt.h");
}
