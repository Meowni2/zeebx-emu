//! Alocador do heap do guest.
//!
//! Serve o `MALLOC`/`FREE` do BREW. A contabilidade fica toda no host — nenhum cabeçalho é
//! escrito na memória do guest — porque o jogo nunca inspeciona a estrutura interna do heap.

use std::collections::{BTreeMap, HashMap};

/// Alinhamento dos blocos. O ARM exige 4; usamos 8 para não penalizar acessos de 64 bits.
const ALIGN: u32 = 8;

#[derive(Debug)]
pub struct Heap {
    base: u32,
    end: u32,
    /// Primeiro endereço nunca usado.
    next: u32,
    /// Blocos devolvidos por [`Heap::free`], do endereço para o tamanho.
    ///
    /// É mapa ordenado, e não lista, porque o que importa aqui é achar o vizinho de um bloco
    /// para fundir os dois — ver [`Heap::free`].
    free_list: BTreeMap<u32, u32>,
    /// Blocos em uso, para que `free` saiba o tamanho sem o chamador informar.
    live: HashMap<u32, u32>,
}

impl Heap {
    pub fn new(base: u32, size: usize) -> Self {
        Self {
            base,
            end: base + size as u32,
            next: base,
            free_list: BTreeMap::new(),
            live: HashMap::new(),
        }
    }

    /// Reserva `size` bytes e devolve o endereço, ou `None` se o heap encheu.
    ///
    /// Um `size` zero ainda recebe endereço próprio: o BREW devolve ponteiro válido nesse caso,
    /// e devolver o mesmo endereço duas vezes confundiria o `free`.
    pub fn alloc(&mut self, size: u32) -> Option<u32> {
        let total = size.max(1).div_ceil(ALIGN) * ALIGN;

        // Primeiro ajuste entre os blocos livres.
        if let Some((&addr, &len)) = self.free_list.iter().find(|&(_, &len)| len >= total) {
            self.free_list.remove(&addr);
            // O resto do bloco volta para a lista em vez de virar desperdício.
            if len > total {
                self.free_list.insert(addr + total, len - total);
            }
            self.live.insert(addr, total);
            return Some(addr);
        }

        let addr = self.next;
        if addr.checked_add(total)? > self.end {
            return None;
        }
        self.next = addr + total;
        self.live.insert(addr, total);
        Some(addr)
    }

    /// Tamanho de um bloco vivo — usado pelo `realloc` para saber quanto copiar.
    pub fn size_of(&self, ptr: u32) -> Option<u32> {
        self.live.get(&ptr).copied()
    }

    /// Devolve um bloco. Retorna o tamanho liberado, ou `None` se o ponteiro não estava vivo
    /// — inclusive o ponteiro nulo, que `FREE` aceita sem fazer nada.
    ///
    /// **O bloco é fundido com os vizinhos livres.** Sem isso o heap se despedaça: cada troca
    /// de tela do Treino Cerebral devolve centenas de blocos pequenos, nenhum deles volta a
    /// formar uma região grande, e o pedido de um mega que vem na fase seguinte não acha onde
    /// caber mesmo com dezenas de megas livres — o jogo não confere o ponteiro nulo e morre
    /// chamando um método em zero.
    pub fn free(&mut self, ptr: u32) -> Option<u32> {
        let size = self.live.remove(&ptr)?;
        let (mut inicio, mut fim) = (ptr, ptr + size);
        // O vizinho de baixo, se ele termina exatamente onde este começa.
        if let Some((&antes, &len)) = self.free_list.range(..inicio).next_back()
            && antes + len == inicio
        {
            self.free_list.remove(&antes);
            inicio = antes;
        }
        // E o de cima, se começa exatamente onde este termina.
        if let Some(&len) = self.free_list.get(&fim) {
            self.free_list.remove(&fim);
            fim += len;
        }
        // Um bloco que encosta no topo não precisa de lista: o `next` recua e ele vira espaço
        // novo de novo, do tamanho que for pedido depois.
        if fim == self.next {
            self.next = inicio;
        } else {
            self.free_list.insert(inicio, fim - inicio);
        }
        Some(size)
    }

    /// Quanto do heap está entregue ao jogo, em bytes.
    ///
    /// Não é `next - base`: o que está na lista de livres já voltou, e contá-lo faria o
    /// `GetRAMFree` mentir para quem pergunta antes de alocar.
    pub fn used(&self) -> u32 {
        let livres: u32 = self.free_list.values().sum();
        (self.next - self.base).saturating_sub(livres)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aloca_sequencialmente_e_alinhado() {
        let mut heap = Heap::new(0x1000, 0x1000);
        let a = heap.alloc(4).unwrap();
        let b = heap.alloc(4).unwrap();
        assert_eq!(a, 0x1000);
        assert_eq!(b, 0x1008, "blocos devem respeitar o alinhamento de {ALIGN}");
    }

    #[test]
    fn reaproveita_bloco_liberado() {
        let mut heap = Heap::new(0x1000, 0x1000);
        let a = heap.alloc(32).unwrap();
        assert_eq!(heap.free(a), Some(32));
        assert_eq!(heap.alloc(16), Some(a), "deveria reusar o bloco livre");
        // O restante do bloco continua disponível.
        assert_eq!(heap.alloc(16), Some(a + 16));
    }

    #[test]
    fn free_de_ponteiro_invalido_e_inofensivo() {
        let mut heap = Heap::new(0x1000, 0x1000);
        assert_eq!(heap.free(0), None);
        assert_eq!(heap.free(0xdead), None);
    }

    #[test]
    fn nao_entrega_dois_ponteiros_iguais_para_tamanho_zero() {
        let mut heap = Heap::new(0x1000, 0x1000);
        assert_ne!(heap.alloc(0).unwrap(), heap.alloc(0).unwrap());
    }

    #[test]
    fn blocos_vizinhos_liberados_voltam_a_ser_um_so() {
        // Sem fundir, três blocos de 32 devolvidos viram três buracos de 32 e um pedido de 96
        // vai para o topo do heap. É assim que o Treino Cerebral esgotava 64 MB.
        let mut heap = Heap::new(0x1000, 0x1000);
        let a = heap.alloc(32).unwrap();
        let b = heap.alloc(32).unwrap();
        let c = heap.alloc(32).unwrap();
        let depois = heap.alloc(8).unwrap();
        heap.free(b);
        heap.free(a);
        heap.free(c);
        assert_eq!(heap.alloc(96), Some(a), "os três deveriam ter virado um só");
        assert_eq!(heap.free(depois), Some(8));
    }

    #[test]
    fn o_bloco_do_topo_devolve_o_espaco_ao_heap() {
        let mut heap = Heap::new(0x1000, 64);
        let a = heap.alloc(32).unwrap();
        heap.free(a);
        assert_eq!(heap.used(), 0, "nada continua entregue");
        assert!(heap.alloc(64).is_some(), "o heap inteiro voltou a caber");
    }

    #[test]
    fn o_uso_nao_conta_o_que_ja_voltou() {
        let mut heap = Heap::new(0x1000, 0x1000);
        let a = heap.alloc(64).unwrap();
        let _b = heap.alloc(64).unwrap();
        assert_eq!(heap.used(), 128);
        heap.free(a);
        assert_eq!(heap.used(), 64, "o bloco livre no meio não está entregue");
    }

    #[test]
    fn recusa_quando_o_heap_enche() {
        let mut heap = Heap::new(0x1000, 16);
        assert!(heap.alloc(8).is_some());
        assert!(heap.alloc(8).is_some());
        assert_eq!(heap.alloc(8), None);
    }
}
