# Status geral
Versão considerada na análise: v0.1.1

| Status | Legenda | Jogos | % |
|---|---|---|---|
| ✅ | (compatível) | 24 | 40% |
| 🔄 | (roda com ressalvas) | 15 | 25% |
| ❌ | (incompatível) | 21 | 35% |


# Observações:

- É recomendado configurar o controle ao abrir o emulador pela primeira vez, mesmo assim, alguns games podem apresentar inputs diferentes.
- Jogos compatíveis com Boomerang (Sensor de movimento), como Crash Nitro Kart 3D, todos os Zeebo Sports, entre outros, podem ser jogados utilizando Wii Remote por meio de pareamento Bluetooth.
- Todos os jogos da Data East são jogos arcade em um emulador embutido próprio, todos funcionam porém ainda sem áudio.
- Nenhum jogo está livre de bugs ou crashes, agradeço seus reports!
- Único jogo homebrew testado e funcionando é o Zeetris, demais brews e ports podem ser ainda incompatíveis.
- Funcionalidades online do Zeeboids podem não estar em operação a todo momento, visto que ainda é um recurso em desenvolvimento. Todos os avatares Zeeboids poderão ser excluídos após o desenvolvimento e migração para um servidor final adequado, mas fique à vontade para testar.

# Lista de Compatibilidade

| Nome do jogo | Compatibilidade | Observações |
|---|---|---|
| Action Hero 3D - Wild Dog and IMICRO3D | ❌ | o jogo chamou ITransform::TransformBltSimple, que ainda não existe aqui (de 0x00018558) |
| Alice no Pais das Maravilhas | ❌ | acesso inválido 🔄 0x0000000c, em 0x000851cc |
| Alien Breaker Deluxe | ❌ | |
| Alpine Racer | ✅ | Poucos problemas visuais e de som, mas completamente jogável |
| Armageddon Squadron | 🔄 | Jogável, porém sem som |
| Bad Dudes vs. DragonNinja | ❌ | Travado no menu |
| Bejeweled Twist | ❌ | acesso inválido 🔄 0x00000024, em 0x0003ab20 |
| Caveman Ninja | 🔄 | Jogável, porém sem som |
| Crash Bandicoot Nitro Kart 3D | ✅ |  |
| Dark Seal | 🔄 | Jogável, porém sem som |
| Disney All Star Cards | ❌ | Crasha após o menu |
| Double Dragon | ✅ | |
| FIFA 09 | ✅ | |
| Galaxy on Fire | 🔄 | Jogável, porém sem som e com inputs incorretos |
| Heavy Barrel | ❌ |  |
| Heavy Weapon | ❌ | Imagens lotadas de glitch |
| Iron Sight | ❌ | acesso inválido 🔄 0x00000010, em 0x00036060 |
| Karnovs Revenge | 🔄 | Jogável, porém sem som |
| Magical Drop 3 | ✅ | Jogável porém sem som |
| Need For Speed - Carbon - Domine 🔄 Cidade | 🔄 | Jogável porém requer otimizacão (esse realmente PRECISA DE VELOCIDADE) |
| Pac-Mania | ✅ | |
| Peggle | ❌ | exceção do núcleo ARM em 0x000108f4 |
| Powerboat Challenge | ❌ | |
| Prey 2 Evil | ❌ | |
| Quake | 🔄 | Podem existir crashes in-game ou lagging |
| Quake 2 | ❌ | acesso inválido 🔄 0x00000000, em 0x00000000 |
| Raging Thunder 2 | 🔄 | Jogável, porém sem som |
| Rally Master Pro | ✅ | |
| Reckless Racing | 🔄 | Abre, porém com uma série de glitches nos modelos e texturas |
| Resident Evil 4 - Zeebo Edition | ✅ | Reportado pela comunidade: Falta fog em algumas cenas do jogo |
| Ridge Racer | ❌ | Abre mas completamente bugado e injogável |
| Spin Master | 🔄 | Jogável, porém sem som |
| Street Hoop | 🔄 | Jogável, porém sem som |
| Super BurgerTime | 🔄 | Jogável, porém sem som |
| Tekken 2 | ✅ | |
| Tork and Kral - 🔄 Prehistorik Adventure | ❌ | Abre todo bugado |
| Toy Raid | 🔄 | Jogável, porém sem som e com glitches visuais |
| Treino Cerebral | ✅ | |
| Turma da Monica em Vamos Brincar Vol. 1 | ❌ | acesso inválido 🔄 0x00e60125, em 0x00060338 |
| Ultimate Chess 3D | ✅ | |
| Um Jogo de Ovos | ❌ | Não detecta controles |
| Wizard Fire | 🔄 | Jogável, porém sem som |
| Zeebo Clube | ❌ | Envolve sistema online, recriacão pendente |
| Zeebo Extreme Baja | ✅ | |
| Zeebo Extreme Boia Cross | ✅ | |
| Zeebo Extreme Corrida Aerea | ✅ | |
| Zeebo Extreme Jetboard | ❌ | |
| Zeebo Extreme Rolima | ❌ | |
| Zeebo Family Pack | ✅ | |
| Zeebo F.C. Foot Camp | ✅ | |
| Zeebo F.C. Super League | ✅ | Herda funcionalidades online do Zeeboids |
| Zeeboids | ✅ | Funcionalidades online 50% |
| Zeebo Sports Peteca | ✅ | |
| Zeebo Sports Queimada | ✅ | |
| Zeebo Sports Tenis | ✅ | |
| Zeebo Sports Volei | ✅ | |
| Zeetris | ✅ | |
| Zenonia | ✅ | |
| Zumas Revenge | ❌ | acesso inválido 🔄 0x0000000c, em 0x00046b9c |
| Z-Wheel | 🔄 | Utilizável, porém ainda com muitos bugs, online não implementado |

