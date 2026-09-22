//! Zeebx sem interface: o emulador por linha de comando, configurado por um `config.ini`.
//!
//! Existe para quem já tem um frontend. Um programa que simula a carcaça do console, uma
//! estante de jogos, um arcade de fliperama — nenhum deles quer a biblioteca do Zeebx por cima
//! da tela que ele mesmo desenhou. Este binário abre o jogo que lhe derem, obedece ao arquivo
//! de configuração e não mostra mais nada.
//!
//! **O núcleo é o mesmo.** Gráficos, áudio e mapeamento de controle são os campos que a
//! interface do desktop grava no `settings.json`; aqui eles chegam por INI, que é o que se
//! edita à mão. Ver [`config`].

mod config;
mod console;
mod despejo;
mod entrada;
mod ini;
mod janela;

use std::path::PathBuf;
use std::process::ExitCode;

use crate::config::Video;
use crate::console::{Console, Fim};

const AJUDA: &str = "\
Zeebx sem interface — o emulador do Zeebo por linha de comando.

  zeebx-headless [OPÇÕES] JOGO

O JOGO é obrigatório: um `.mod`, ou o `.zip` que o contém. Para abrir pela
Z-Wheel, passe a Z-Wheel — ela é um jogo como outro qualquer.

Opções:
  --config=CAMINHO   o arquivo de configuração a usar. Sem isto, procura um
                     `config.ini` ao lado do executável e depois na pasta de
                     configuração do sistema.
  --controles        lista os controles que o sistema enxerga, com os nomes que
                     `[porta1] controle` espera, e sai.
  --exemplo          escreve na saída padrão um `config.ini` comentado, com os
                     valores de fábrica, e sai.
  --ajuda            isto.
  --versao           a versão.

Tudo o mais — vídeo, áudio, controles — está no `config.ini`.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--ajuda" || a == "-h" || a == "--help") {
        print!("{AJUDA}");
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--versao" || a == "--version") {
        println!("zeebx-headless {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--exemplo") {
        print!("{}", include_str!("../config.ini"));
        return ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--controles") {
        return lista_controles();
    }

    let arquivo = args
        .iter()
        .find_map(|a| a.strip_prefix("--config="))
        .map(PathBuf::from);
    // O que não começa com `--` é o jogo. Um só: abrir dois não quer dizer nada.
    let jogo = args.iter().find(|a| !a.starts_with("--")).map(PathBuf::from);
    if let Some(desconhecida) = args
        .iter()
        .find(|a| a.starts_with("--") && !a.starts_with("--config="))
    {
        eprintln!("erro: não conheço `{desconhecida}`. `--ajuda` lista o que existe.");
        return ExitCode::FAILURE;
    }

    let lido = config::carrega(arquivo.as_deref());
    for aviso in &lido.avisos {
        eprintln!("config: {aviso}");
    }
    let origem = lido.origem;
    let modo = lido.headless.video;
    let console = Console::novo(lido.settings, lido.headless);

    // Primeira execução: deixa um `config.ini` de fábrica escrito, comentado linha a linha, em
    // vez de só rodar com os padrões e não dizer onde se muda nada.
    if let config::Origem::Faltando(onde) = &origem {
        match config::cria(onde) {
            Ok(()) => eprintln!("config: não havia nenhum; escrevi um em {}", onde.display()),
            Err(erro) => eprintln!("config: sem arquivo, valendo as opções de fábrica ({erro})"),
        }
    }

    // **O jogo é obrigatório, e não tem padrão.** Este binário é chamado por outro programa,
    // que sabe o que quer abrir; adivinhar alguma coisa quando ele não diz nada seria abrir o
    // que ninguém pediu. Quem quiser a Z-Wheel a passa como qualquer outro jogo.
    let Some(caminho) = jogo else {
        eprintln!(
            "erro: falta dizer o que rodar.\n\
             \n\
             Passe o `.mod`, ou o `.zip` que o contém:\n\
             \x20   zeebx-headless CAMINHO/DO/JOGO.zip"
        );
        return ExitCode::FAILURE;
    };

    match modo {
        Video::Nenhum => sem_janela(console, &caminho),
        _ => janela::roda(console, caminho),
    }
}

/// O modo sem vídeo: nada na tela, e o quadro sai pelo `[despejo]`.
fn sem_janela(mut console: Console, caminho: &std::path::Path) -> ExitCode {
    // Sem janela não há contexto de GL vindo de uma, mas o 3D na placa continua possível: o
    // núcleo sabe abrir um contexto fora de tela, que é o que o `zeebx run` já usa para medir.
    let contexto = match console.settings.graphics.gpu_rasterizer {
        true => match zeebx::video::contexto::Contexto::novo() {
            Ok(contexto) => Some(contexto),
            Err(erro) => {
                eprintln!("aviso: sem placa alcançável, o 3D fica em software: {erro}");
                None
            }
        },
        false => None,
    };
    console.usa_contexto(contexto.as_ref().map(|c| c.gl.clone()));

    let mut saida = match despejo::Saida::abre(&console.opcoes.despejo.clone()) {
        Ok(saida) => saida,
        Err(erro) => {
            eprintln!("erro: {erro}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(erro) = console.abre(caminho) {
        eprintln!("erro: {}: {erro}", caminho.display());
        return ExitCode::FAILURE;
    }

    // O quadro só é escrito quando muda: o laço do emulador gira muito mais vezes que o jogo
    // desenha, e repetir o mesmo quadro encheria o cano de cópias idênticas.
    let mut visto = None;
    loop {
        match console.passo() {
            Fim::Segue => {}
            Fim::Acabou(motivo) => {
                eprintln!("parou: {motivo}");
                return ExitCode::SUCCESS;
            }
            Fim::Erro(erro) => {
                eprintln!("erro: {erro}");
                return ExitCode::FAILURE;
            }
        }
        if console.adiantado() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let grande = console.opcoes.despejo.formato;
        let Some(sessao) = console.sessao_mut() else {
            return ExitCode::SUCCESS;
        };
        let agora = {
            let tela = sessao.screen();
            (tela.serie(), tela.escritas())
        };
        if visto == Some(agora) {
            continue;
        }
        visto = Some(agora);
        // **Os formatos crus saem sempre no quadro do console, e só o `.png` acompanha a
        // resolução interna.** Um fluxo de bytes sem cabeçalho tem de ter o quadro do mesmo
        // tamanho do começo ao fim: o 3D na placa sai grande e o menu 2D sai pequeno, e quem
        // conta bytes do outro lado perderia o passo na primeira troca. O `.png` diz o próprio
        // tamanho em cada arquivo, então ali a imagem maior não atrapalha ninguém.
        let quadro = match grande == config::Formato::Png {
            true => sessao.quadro_grande(),
            false => None,
        };
        let quadro = quadro.as_ref().unwrap_or_else(|| sessao.screen());
        if let Err(erro) = saida.escreve(quadro) {
            // A outra ponta fechou o cano. Não é falha de quem rodou: é o fim normal de
            // `zeebx-headless ... | frontend` quando o frontend sai.
            eprintln!("parou: {erro} ({} quadros)", saida.contados());
            return ExitCode::SUCCESS;
        }
        if saida.cheia() {
            eprintln!("parou: os {} quadros pedidos saíram", saida.contados());
            return ExitCode::SUCCESS;
        }
    }
}

/// Os controles que o sistema enxerga, pelos nomes que o `config.ini` espera.
fn lista_controles() -> ExitCode {
    let pads = zeebx::input::gamepads::Gamepads::new();
    let nomes = pads.names();
    if nomes.is_empty() {
        println!("nenhum controle visto pelo sistema");
        return ExitCode::SUCCESS;
    }
    println!("controles vistos, na ordem do sistema:");
    for (i, nome) in nomes.iter().enumerate() {
        println!("  {i}: {nome}");
    }
    println!("\nem [porta1] do config.ini, por exemplo:\n  controle = \"{}\"", nomes[0]);
    ExitCode::SUCCESS
}
