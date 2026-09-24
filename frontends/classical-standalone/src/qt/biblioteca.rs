//! A biblioteca de jogos, como modelo de lista para o QML.
//!
//! Por enquanto é a lista e a abertura. A grade, o slider, as capas e a busca são da fase 4 de
//! `docs/implementacao/21-migracao-para-qt.md`; o que já está aqui é o que elas vão usar.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++Qt" {
        include!(<QtCore/QAbstractListModel>);
        #[qobject]
        type QAbstractListModel;
    }

    unsafe extern "C++" {
        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;
        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;
        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        type Biblioteca = super::BibliotecaRust;

        #[qinvokable]
        #[cxx_override]
        fn data(self: &Biblioteca, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "rowCount"]
        fn row_count(self: &Biblioteca, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        #[cxx_name = "roleNames"]
        fn role_names(self: &Biblioteca) -> QHash_i32_QByteArray;

        /// Abre o jogo da linha dada. Devolve vazio, ou o motivo de não ter aberto.
        #[qinvokable]
        fn abre(self: &Biblioteca, linha: i32) -> QString;

        /// Abre a Z-Wheel. Devolve vazio, ou o motivo de não ter aberta.
        #[qinvokable]
        #[cxx_name = "abreZWheel"]
        fn abre_z_wheel(self: &Biblioteca) -> QString;

        /// Abre o jogo de um caminho — o que veio na linha de comando.
        #[qinvokable]
        #[cxx_name = "abreCaminho"]
        fn abre_caminho(self: &Biblioteca, caminho: &QString) -> QString;

        /// Se há Z-Wheel para o botão abrir.
        #[qinvokable]
        #[cxx_name = "temZWheel"]
        fn tem_z_wheel(self: &Biblioteca) -> bool;

        /// Como a janela principal abre: 0 em janela, 1 maximizada, 2 em tela cheia. Ver
        /// `graphics.janela`.
        #[qinvokable]
        #[cxx_name = "modoDaJanela"]
        fn modo_da_janela(self: &Biblioteca) -> i32;

        /// A pasta de ROMs configurada, para dizer de onde a lista veio.
        #[qinvokable]
        fn pasta(self: &Biblioteca) -> QString;
    }
}

use std::path::{Path, PathBuf};

use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};

use super::nucleo;

/// O primeiro papel livre para o modelo: abaixo dele ficam os do Qt, como o `DisplayRole`.
const USER_ROLE: i32 = 0x0100;
const TITULO: i32 = USER_ROLE;
const CAMINHO: i32 = USER_ROLE + 1;

#[derive(Default)]
pub struct BibliotecaRust;

/// O resultado de uma abertura, como o QML o lê: vazio é sucesso.
fn resposta(aberta: Result<(), String>) -> QString {
    match aberta {
        Ok(()) => QString::default(),
        Err(erro) => QString::from(&erro),
    }
}

impl qobject::Biblioteca {
    pub fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        nucleo::com(|nucleo| {
            let Some(jogo) = usize::try_from(index.row())
                .ok()
                .and_then(|linha| nucleo.lista.get(linha))
                .map(|&indice| &nucleo.jogos[indice])
            else {
                return QVariant::default();
            };
            match role {
                TITULO => QVariant::from(&QString::from(&jogo.title)),
                CAMINHO => QVariant::from(&QString::from(&jogo.path.display().to_string())),
                _ => QVariant::default(),
            }
        })
    }

    pub fn row_count(&self, _parent: &QModelIndex) -> i32 {
        nucleo::com(|nucleo| nucleo.lista.len() as i32)
    }

    pub fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut papeis = QHash::<QHashPair_i32_QByteArray>::default();
        papeis.insert(TITULO, QByteArray::from("titulo"));
        papeis.insert(CAMINHO, QByteArray::from("caminho"));
        papeis
    }

    pub fn abre(&self, linha: i32) -> QString {
        resposta(nucleo::com(|nucleo| {
            let caminho = usize::try_from(linha)
                .ok()
                .and_then(|linha| nucleo.lista.get(linha))
                .map(|&indice| nucleo.jogos[indice].path.clone())
                .ok_or_else(|| "esse jogo não está na lista".to_string())?;
            nucleo.abre_pela_biblioteca(&caminho)
        }))
    }

    pub fn abre_z_wheel(&self) -> QString {
        resposta(nucleo::com(|nucleo| {
            let caminho = nucleo
                .z_wheel
                .clone()
                .ok_or_else(|| "a Z-Wheel não está configurada".to_string())?;
            nucleo.abre_pela_biblioteca(&caminho)
        }))
    }

    pub fn abre_caminho(&self, caminho: &QString) -> QString {
        let caminho = PathBuf::from(String::from(caminho));
        let caminho = std::fs::canonicalize(&caminho).unwrap_or(caminho);
        resposta(nucleo::com(|nucleo| nucleo.abre_pela_biblioteca(Path::new(&caminho))))
    }

    pub fn tem_z_wheel(&self) -> bool {
        nucleo::com(|nucleo| nucleo.z_wheel.is_some())
    }

    pub fn modo_da_janela(&self) -> i32 {
        nucleo::com(|nucleo| super::ponte::modo(nucleo.settings.graphics.janela))
    }

    pub fn pasta(&self) -> QString {
        nucleo::com(|nucleo| {
            let pasta = nucleo.settings.roms_dir.as_deref();
            QString::from(&pasta.map(|p| p.display().to_string()).unwrap_or_default())
        })
    }
}
