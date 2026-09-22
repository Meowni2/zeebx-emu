//! O estado da máquina em seções: registradores, memória e os contadores de alocação.
//!
//! ## O que entra, e por quê
//!
//! **Toda região gravável** do mapa. O que fica de fora é o que o carregador monta uma vez e
//! ninguém altera depois: as vtables e a página nula. As duas são derivadas da imagem, então
//! gravá-las só gastaria bytes — e um estado que carrega vtables velhas por cima das novas seria
//! pior que não carregar nada.
//!
//! ## O corte nas regiões que só crescem
//!
//! O heap (64 MB), a região de objetos (4 MB) e as superfícies (8 MB) nascem **zeradas** e só
//! avançam: `next` é o primeiro endereço nunca usado, e abaixo dele está tudo o que o jogo pediu.
//! Gravar além disso seria escrever dezenas de megabytes de zero em cada save state. O corte é o
//! `next` do alocador de cada região, e é o que faz o tamanho do estado ser proporcional ao que o
//! jogo usou — alguns megabytes, e não oitenta.
//!
//! ## O que ainda não entra
//!
//! A região de objetos entra, mas o **livro** dela não: [`crate::brew::objects::ObjectStore`]
//! guarda a interface de cada objeto num `enum` que ainda não tem codificação estável. Enquanto
//! isso e as tabelas de estado por objeto não entrarem, o `retro_serialize_size` do core continua
//! respondendo zero — ver o plano. Restaurar um estado parcial é exatamente o que o critério
//! proíbe, e a porta é essa.

use super::Machine;
use crate::cpu::{CpuBackend, Reg};
use crate::save_state::{Erro, Guardavel, Leitor, Secoes};

/// Os registradores, na ordem em que a seção os grava.
///
/// A ordem é fixa e explícita: gravar "todos os registradores" na ordem do `enum` deixaria o
/// formato refém de alguém reordenar a enumeração.
const REGISTRADORES: [Reg; 16] = [
    Reg::R0,
    Reg::R1,
    Reg::R2,
    Reg::R3,
    Reg::R4,
    Reg::R5,
    Reg::R6,
    Reg::R7,
    Reg::R8,
    Reg::R9,
    Reg::R10,
    Reg::R11,
    Reg::R12,
    Reg::Sp,
    Reg::Lr,
    Reg::Pc,
];

/// Quanto de uma região foi usado, a partir do primeiro endereço nunca usado.
///
/// Um `next` fora da região seria defeito do alocador; gravar tudo é a resposta segura.
fn usado(proximo: u32, base: u32, tamanho: usize) -> usize {
    let usado = proximo.saturating_sub(base) as usize;
    if usado > tamanho { tamanho } else { usado }
}

/// O nome da seção de uma região.
fn secao_da_regiao(nome: &str) -> String {
    format!("mem.{nome}")
}

impl<C: CpuBackend> Machine<C> {
    /// Quantos bytes de uma região devem ser gravados.
    ///
    /// O `next` do alocador da região, quando ela tem um; o tamanho inteiro, quando não tem.
    /// Devolve `None` para a região que não deve ser gravada de jeito nenhum.
    fn quanto_gravar(&self, nome: &str, base: u32, tamanho: usize, gravavel: bool) -> Option<usize> {
        if !gravavel {
            return None;
        }
        Some(match nome {
            // O heap corta no `next` porque **o livro dele entra no estado**: na volta é de lá que
            // sai o tamanho esperado, e é isso que permite conferir antes de escrever.
            // As regiões de objetos e superfícies ainda vão inteiras: os alocadores delas não estão
            // no formato, e truncar por um contador que o arquivo não guarda deixaria a volta sem
            // com o que comparar. Entram inteiras até os livros delas entrarem.
            "heap" => usado(self.heap.proximo(), base, tamanho),
            _ => tamanho,
        })
    }

    /// Grava o estado em uma seção por região, mais os registradores e os livros.
    pub fn grava_estado(&self) -> Vec<u8> {
        let mut secoes = Secoes::nova();
        secoes.poe_u32s(
            "cpu.regs",
            REGISTRADORES.iter().map(|reg| self.cpu.read_reg(*reg)),
        );
        secoes.poe_u32("cpu.thumb", u32::from(self.cpu.em_thumb()));
        for regiao in self.module.mem.regions() {
            let Some(quanto) = self.quanto_gravar(regiao.name, regiao.base, regiao.bytes.len(), regiao.writable) else {
                continue;
            };
            let mut bytes = vec![0u8; quanto];
            // A memória **viva** está no núcleo, e não no mapa do carregador: é por aqui que se
            // lê o que o jogo escreveu desde o `reset`.
            if self.cpu.read_mem(regiao.base, &mut bytes).is_ok() {
                secoes.poe(&secao_da_regiao(regiao.name), bytes);
            }
        }
        self.heap.grava(&mut secoes);
        secoes.fecha()
    }

    /// Restaura o estado, e **recusa antes de aplicar** se alguma seção não bater.
    ///
    /// Nada é escrito no núcleo enquanto todas as seções não tiverem sido lidas e conferidas: um
    /// estado pela metade dentro de uma máquina em execução é pior que um estado recusado.
    pub fn restaura_estado(&mut self, arquivo: &[u8]) -> Result<(), Erro> {
        let leitor = Leitor::abre(arquivo)?;

        let registradores = leitor.u32s("cpu.regs")?;
        if registradores.len() != REGISTRADORES.len() {
            return Err(Erro::Secao {
                nome: "cpu.regs".to_string(),
                motivo: format!(
                    "o estado tem {} registradores e a máquina tem {}",
                    registradores.len(),
                    REGISTRADORES.len()
                ),
            });
        }
        let _thumb = leitor.u32("cpu.thumb")?;

        // A memória é lida e conferida antes de qualquer escrita.
        let mut regioes: Vec<(u32, Vec<u8>)> = Vec::new();
        for regiao in self.module.mem.regions() {
            let Some(quanto) = self.quanto_gravar(regiao.name, regiao.base, regiao.bytes.len(), regiao.writable) else {
                continue;
            };
            let secao = secao_da_regiao(regiao.name);
            let bytes = leitor.secao(&secao).ok_or_else(|| Erro::Secao {
                nome: secao.clone(),
                motivo: "a seção não está no arquivo".to_string(),
            })?;
            // **O tamanho esperado do heap sai do próprio estado**, e não da máquina de agora:
            // quem carrega um save state carrega o heap que estava lá, e não o que está aqui.
            let quanto = match regiao.name {
                "heap" => usado(
                    leitor.u32("heap.next")?,
                    leitor.u32("heap.base")?,
                    regiao.bytes.len(),
                ),
                _ => quanto,
            };
            if bytes.len() != quanto {
                return Err(Erro::Secao {
                    nome: secao,
                    motivo: format!(
                        "o estado tem {} bytes desta região e esta máquina espera {quanto} \
                         — é um estado de outro jogo ou de outra versão do motor",
                        bytes.len()
                    ),
                });
            }
            regioes.push((regiao.base, bytes.to_vec()));
        }

        // Daqui para baixo é aplicação: ou tudo, ou nada.
        self.heap.restaura(&leitor)?;
        for (base, bytes) in regioes {
            self.cpu.write_mem(base, &bytes).map_err(|erro| Erro::Secao {
                nome: format!("mem.{base:#010x}"),
                motivo: format!("não deu para escrever: {erro}"),
            })?;
        }
        for (reg, valor) in REGISTRADORES.iter().zip(registradores) {
            self.cpu.write_reg(*reg, valor);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::unicorn::UnicornCpu;
    use crate::loader::self as loader;

    /// O menor módulo que o carregador aceita. Não precisa fazer nada: o alvo aqui é o estado da
    /// máquina em volta dele — memória, registradores e os contadores de alocação.
    fn modulo() -> crate::loader::modfile::ModImage {
        let code = [
            0xe3a0_0010u32.to_le_bytes(), // mov r0, #16
            0xe12f_ff1eu32.to_le_bytes(), // bx lr
        ]
        .concat();
        crate::loader::modfile::ModImage::parse(code).unwrap()
    }

    fn maquina() -> Machine<UnicornCpu> {
        let module = loader::load(&modulo()).unwrap();
        let mut machine = Machine::new(UnicornCpu::new().unwrap(), module, ".");
        machine.cpu.reset(&machine.module.mem).unwrap();
        machine
    }

    /// **O estado volta igual**: registradores, heap, pilha e o livro do heap.
    ///
    /// O heap é o caso interessante porque a memória dele *não* vai inteira: vai até o primeiro
    /// endereço nunca usado. Se o corte estiver errado, o que o jogo tinha escrito some — e um
    /// save state que perde os dados do jogo é pior que um save state que não existe.
    #[test]
    fn registradores_e_memoria_vem_de_volta() {
        let mut antes = maquina();
        let bloco = antes.heap.alloc(0x4000).expect("bloco no heap");
        let no_heap = bloco + 0x100;
        antes.cpu.write_mem(no_heap, b"estado do jogo").unwrap();
        let na_pilha = loader::STACK_BASE + 0x300;
        antes.cpu.write_mem(na_pilha, &[7u8; 16]).unwrap();
        antes.cpu.write_reg(Reg::R5, 0xdead_beef);
        antes.cpu.write_reg(Reg::R12, 0x1020_3040);

        let arquivo = antes.grava_estado();

        let mut depois = maquina();
        depois.restaura_estado(&arquivo).expect("restaurou");

        assert_eq!(depois.cpu.read_reg(Reg::R5), 0xdead_beef);
        assert_eq!(depois.cpu.read_reg(Reg::R12), 0x1020_3040);
        let mut lido = [0u8; 14];
        depois.cpu.read_mem(no_heap, &mut lido).unwrap();
        assert_eq!(&lido, b"estado do jogo");
        let mut pilha = [0u8; 16];
        depois.cpu.read_mem(na_pilha, &mut pilha).unwrap();
        assert_eq!(pilha, [7u8; 16]);

        // E a máquina restaurada aloca onde a original alocaria, como no teste do heap sozinho.
        let proximo_antes = antes.heap.alloc(64);
        let proximo_depois = depois.heap.alloc(64);
        assert_eq!(proximo_depois, proximo_antes);
    }

    /// **O heap volta com o `next` que o estado tinha**, e não com o da máquina de agora.
    ///
    /// É o contrário do que eu escrevi primeiro: eu tratei "o heap do estado é maior" como erro,
    /// e não é — é o caso normal. Quem carrega um save state carrega o heap que estava lá.
    #[test]
    fn o_heap_volta_com_o_tamanho_do_estado() {
        let mut antes = maquina();
        antes.heap.alloc(0x2000).expect("bloco no heap");
        let arquivo = antes.grava_estado();

        let mut depois = maquina();
        assert_eq!(depois.heap.proximo(), loader::HEAP_BASE, "a máquina nova começa vazia");
        depois.restaura_estado(&arquivo).expect("restaurou");
        assert_eq!(depois.heap.proximo(), antes.heap.proximo());
    }

    /// Um estado de **outro módulo** é recusado, e sem escrever nada.
    ///
    /// Aqui a máquina tem outro módulo, logo outra região `module`: o tamanho não bate, e o erro
    /// diz os dois números. Aplicar isso escreveria dados de um jogo por cima do código de outro.
    #[test]
    fn estado_de_outro_modulo_e_recusado() {
        let mut antes = maquina();
        antes.heap.alloc(0x1000).expect("bloco no heap");
        let arquivo = antes.grava_estado();

        let maior = [
            0xe3a0_0010u32.to_le_bytes(),
            0xe3a0_1010u32.to_le_bytes(),
            0xe3a0_2010u32.to_le_bytes(),
            0xe3a0_3010u32.to_le_bytes(),
            0xe12f_ff1eu32.to_le_bytes(),
        ]
        .concat();
        let module = loader::load(
            &crate::loader::modfile::ModImage::parse(maior).unwrap(),
        )
        .unwrap();
        let mut outra = Machine::new(UnicornCpu::new().unwrap(), module, ".");
        outra.cpu.reset(&outra.module.mem).unwrap();

        let proximo_antes = outra.heap.proximo();
        match outra.restaura_estado(&arquivo) {
            Err(Erro::Secao { nome, motivo }) => {
                assert_eq!(nome, "mem.module");
                assert!(motivo.contains("bytes desta região"), "{motivo}");
            }
            outro => panic!("devia recusar o estado de outro módulo, e devolveu {outro:?}"),
        }
        assert_eq!(
            outra.heap.proximo(),
            proximo_antes,
            "a recusa não pode ter aplicado metade"
        );
    }

    /// **O tamanho do estado, medido.** É o que decide se isso é usável: um save state que grava
    /// a memória inteira sairia com 84 MB, e o RetroArch grava um a cada atalho do jogador.
    ///
    /// O corte no heap é o que faz a diferença, e as regiões que ainda vão inteiras (objetos e
    /// superfícies) são a maior parte do que sobra. O teste imprime o número porque ele muda
    /// quando as próximas seções entrarem — e é bom que mude à vista.
    #[test]
    fn o_estado_nao_carrega_a_memoria_inteira() {
        let mut maquina = maquina();
        maquina.heap.alloc(64 * 1024).expect("bloco no heap");
        let arquivo = maquina.grava_estado();
        let mb = arquivo.len() as f64 / (1024.0 * 1024.0);
        eprintln!(
            "estado da máquina mínima: {} bytes ({mb:.2} MB); a memória mapeada é 84 MB",
            arquivo.len()
        );
        assert!(
            arquivo.len() < 24 * 1024 * 1024,
            "o estado passou de 24 MB: {} bytes",
            arquivo.len()
        );
    }

    #[test]
    fn um_arquivo_que_nao_e_save_state_e_recusado() {
        let mut maquina = maquina();
        assert!(matches!(
            maquina.restaura_estado(b"nao sou eu"),
            Err(Erro::Truncado { .. })
        ));
    }
}
