// A cola em C++ que o cxx-qt-lib não cobre: a preparação do GL antes do QGuiApplication e o
// contexto fora de tela que o rasterizador na placa recebe emprestado.
#pragma once

#include <cstddef>

#include "rust/cxx.h"

namespace zeebx {

// Antes do QGuiApplication: GL como API do Qt Quick, 3.3 core, contextos compartilhados e o
// render loop `basic`, que desenha na thread principal — onde o emulador roda.
void prepara_gl();

// O contexto do rasterizador: próprio, fora de tela e compartilhado com o do Qt Quick. Depois
// do QGuiApplication. Devolve falso se a placa não der.
bool gl_cria();
bool gl_torna_corrente();
void gl_solta();

// O endereço de uma função de GL, para o `glow`. Zero se não houver.
std::size_t gl_funcao(rust::Str nome);

} // namespace zeebx
