# Kestrel — handoff para o Claude Design

## O produto
Kestrel é um otimizador de FPS para Windows, grátis e open source, para jogadores competitivos.
Começa pelo VALORANT; depois vêm FiveM, Rainbow Six Siege e Warzone, cada um como um "perfil de
jogo". É um app desktop (Tauri): janela sem moldura de 1180×760 (mínimo 1024×680), roda como admin.
Os textos da interface ficam em inglês (traduzíveis depois).

## Identidade (já decidida — manter)
- Estilo tático. Fundo quase preto #0C0E11; painéis #15181D e #1B1F25; linhas #232830 e #3A414B.
- Texto #E9ECEF; secundário #8C949E. Um único acento verde-limão #C8F53C (texto sobre ele: #0C0E11).
  Aviso #FFB547, perigo #FF6B6B.
- Fontes: Chakra Petch (interface) e JetBrains Mono (números, sempre tabulares).
- Cantos chanfrados (corte de ~10px nos painéis, ~5px em botões e tags), sem cantos arredondados.
- Abas no topo. Logo "Hover": falcão visto de frente, 3 polígonos, verde-limão.

## Telas, por prioridade
1. Barra de título (arrastável): logo + "Kestrel", abas Home · Health · Hardware · Tweaks · Lock In ·
   Benchmark · Toolbox, selo "admin", botões de janela.
2. Home, status primeiro: hero com fundo animado + System score 0–100 (barra de 20 segmentos),
   "N fixes found", os 3 principais problemas com botão de ação, "Apply plan" e "See all checks".
   Cards: Hardware, Lock In (interruptor), Live load (gráfico de 60 s de CPU/GPU), Last benchmark.
   Rodapé: Copy diagnostics, Scan again, Help on GitHub, aviso "not affiliated with Riot Games".
3. Primeira execução guiada: boas-vindas → "Scanning" animado → "Here's what Kestrel found" →
   oferta do benchmark → Home.
4. Health Check: 8 grupos, 27 verificações; status Pass/Problem/Info/Unknown, gravidade
   High/Med/Low, "How to fix" que expande.
5. Benchmark: rota da Range (aquecimento + 3 voltas de 90 s iniciadas com F10, com bipes), estado da
   sessão, resultados (FPS médio, 1% low, 0.1% low, travadas por minuto, PC latency, gargalo CPU ou
   GPU, gráfico de frame time) e comparação com o "Starting point" (mudança real ou ruído).
6. Hardware (estilo CPU-Z): processador, placa-mãe/BIOS, memória pente por pente, placa de vídeo
   com link PCIe.
7. Toolbox: guia da BIOS para a placa do usuário (seção em foco marcada com barra verde).
8. Depois: Tweaks (níveis Essential/Competitive/Extreme com Preview, Undo e "Relatório do Kestrel"),
   Lock In, e um seletor de jogo (VALORANT, FiveM, R6, Warzone).

## Movimento
- Animações em tudo, mas leves: CSS transform/opacity e canvas simples, sem vídeo.
- Elas pausam sozinhas com um jogo aberto ou a janela minimizada, respeitam "reduzir animações" do
  Windows e têm um interruptor nas configurações. Meta: quase 0% de CPU parado.
- Ideias já exploradas (prévia: https://claude.ai/artifact/X8mtnmSssmtxa9RE6KTkVT):
  falcão em partículas como hero da Home; radar tático como animação de "scanning"; correntes de
  vento. Micro-interações: brilho que varre o botão principal, cantos de mira no secundário,
  indicador de aba que desliza, interruptor com mola, placar contando até a nota.

## Restrições
- React + CSS; fontes empacotadas no app (sem internet em tempo de execução).
- Foco de teclado visível (o anel fica por dentro dos cortes), bom contraste.
- Prever estados: carregando, erro, vazio e dados de exemplo.

## Dados reais para os mockups
Score 97/100, um problema: BIOS 2203 de 2024-02-07 (Low). i9-11900K 8C/16T, RTX 3060 Ti com driver
610.74, 32 GB DDR4 a 3600 MT/s, monitores de 360 Hz e 180 Hz, Windows 11 Pro 25H2,
ROG MAXIMUS XIII HERO.

## O que devolver
Telas em 1180×760 (e 1024×680); tokens (cores, tipografia, espaçamento, cortes, brilhos, durações e
curvas de animação); componentes (botões, abas, tags, cards, interruptores, gráficos, linhas de
verificação); a especificação das animações (o que anima, duração, curva, quando pausa); o hero
da Home.
