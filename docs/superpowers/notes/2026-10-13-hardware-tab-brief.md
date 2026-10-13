# Kestrel — tela Hardware para o Claude Design

## O pedido
Adicione uma tela **Hardware** ao "Kestrel Redesign v4", na mesma linguagem visual: trilho de ícones,
cabeçalhos numerados ("01 …"), valores em JetBrains Mono, cortes chanfrados, limão para "bom" e
"ao vivo", âmbar para problema e cinza para "Not reported". Ela entra no trilho como **03 Hardware**,
entre Health e Tweaks; as outras abas passam a 04–07. Título da página: "Hardware".

## O que a tela faz
- Lista cada peça do PC que o Windows informa sem driver: processador, placa de vídeo, memória,
  placa-mãe e BIOS, monitores, discos e rede.
- Mostra o clock de cada núcleo do processador ao vivo, uma vez por segundo.
- Marca as configurações da BIOS que custam FPS e leva ao passo a passo no guia da BIOS (Toolbox).
- "Copy specs" copia um resumo em texto para colar no Discord ou num fórum.
- Tudo é só leitura. Os dados vêm do último scan; só os clocks se mexem.

Os valores abaixo são do PC do dono e servem de exemplo. No app, todos vêm do Kestrel; nenhum é
fixo no código.

## Topo da página
- Resumo em uma linha: `Core i9-11900K · RTX 3060 Ti · 32 GB DDR4-3600 · ROG MAXIMUS XIII HERO`
- `Windows 11 Pro 25H2 · build 26200`
- `LAST SCAN 13 OCT 21:04` e o botão "Scan again"
- Botão "Copy specs": por 3 s vira "Copied to clipboard" / "PASTE IT ANYWHERE", como o Copy
  diagnostics.

## Seções, nesta ordem

### 01 Processor
- `Intel Core i9-11900K` · codinome `Rocket Lake-S` · soquete `LGA1200`
- `8 cores · 16 threads`. Em processadores híbridos: `8 P-cores + 16 E-cores · 32 threads`.
- `Base 3.50 GHz` · `Max boost 5.30 GHz`. Em AMD o máximo pode faltar: "Not reported".
- Caches: `L1 data 8 × 48 KB` · `L1 instruction 8 × 32 KB` · `L2 8 × 512 KB` · `L3 16 MB`
- Instruções, em chips: `SSE4.2` `AVX` `AVX2` `AVX-512`. As que o processador não tem ficam apagadas.
- **Live clocks**, ao vivo a 1 Hz: uma célula ou barra por núcleo com o clock atual (`C0 4.80 GHz` …
  `C7 5.20 GHz`), mais `Average 4.86 GHz` e `Highest 5.20 GHz`. Em processadores híbridos, cada
  núcleo vem marcado P ou E. Selo "1 Hz"; estado "PAUSED" com a janela em segundo plano ou um jogo em
  primeiro plano, como o Live load da Home.

### 02 Graphics
Um card por placa de vídeo. Adaptadores virtuais (por exemplo o Parsec) ficam numa linha discreta no
fim.
- `NVIDIA GeForce RTX 3060 Ti` · `8 GB` · driver `610.74` (`2026-09-15`)
- **PCIe link**
  - `Card 4.0 x16` · `Slot 4.0 x16` (o que a placa e o slot suportam)
  - `Now 1.0 x16`, com a nota "The card slows its link when idle."
  - `Under load 4.0 x16`, com "From your benchmark on 13 Oct". Sem benchmark ainda: "Run a benchmark
    to check the link under load."
- **Resizable BAR**: `On`, `Off` ou `Not reported`
- Um problema (por exemplo `x8` numa placa x16, ou `3.0` sob carga num slot 4.0) aparece em âmbar,
  com o link "BIOS guide".

### 03 Memory
- Resumo: `32 GB DDR4` · `Dual channel` · `2 of 4 slots` · `3600 MT/s now` · `Kit rated 3600 MT/s`.
  A nota do kit vem do part number; quando o Kestrel não reconhece: "Rating unknown".
- Uma linha por pente: slot `A2` · `16 GB` · `Corsair` · `CMG32GX4M2D3600C18` · `3600 MT/s` ·
  `Dual rank`
- Nota fixa: "Timings and voltages need a driver, which Kestrel never loads."

### 04 Motherboard and BIOS
- `ASUS` · `ROG MAXIMUS XIII HERO` · chipset `Intel Z590`
- BIOS `American Megatrends` · versão `2103` · `2024-05-20`. BIOS antiga: âmbar, com o link
  "BIOS guide".
- **BIOS settings Kestrel can see**: uma lista de verificação. Cada item tem um estado e, quando está
  desligado ou errado, o link "BIOS guide" para o tópico certo:
  - `XMP` On ("RAM runs at its rated 3600 MT/s")
  - `Resizable BAR` On
  - `Virtualization (VT-x)` On
  - `IOMMU (VT-d)` Off, com a nota "Only for Vanguard On-Demand"
  - `Secure Boot` On
  - `TPM 2.0` On
  - `Boot mode` UEFI

### 05 Displays
Um card por monitor (nomes de exemplo):
- `ZOWIE XL2566K` · `1920×1080` · `360 Hz of 360 Hz` · `Primary` · ligado na `RTX 3060 Ti`
- `AOC 24G4` · `1920×1080` · `180 Hz of 180 Hz`

Um monitor abaixo da própria taxa máxima aparece em âmbar.

### 06 Storage
Um card por disco:
- `Samsung SSD 980 PRO 1TB` · `NVMe` · `931 GB`. Volume `C:` com `210 GB free of 931 GB` e a tag
  `VALORANT` (os jogos instalados nesse disco).
- `WDC WD20EZBX` · `HDD` · `1.82 TB`. Volume `D:` com `1.20 TB free of 1.82 TB`.

### 07 Network
Um card por adaptador: `Intel Ethernet Controller I225-V` · `Ethernet` · `2.5 Gbps` · `Connected` ·
`Power saving On`. Esse último fica em âmbar, porque é um achado do Health Check.

## Estados
- "Not reported", em cinza, quando o Windows não informa um valor. Nada é inventado.
- Aviso na seção quando a leitura falha: "Kestrel couldn't read everything here. Use Copy
  diagnostics on Home to share the details."
- Antes do primeiro scan, a tela pede o scan.
- Clocks pausados: o último valor fica apagado, com "PAUSED".
- Com "reduzir animações" ligado no Windows, os clocks mudam sem animação.

## O que fica de fora
Temperatura do processador, voltagens, timings da RAM, ventoinhas e limites de energia. Tudo isso
precisa de um driver no kernel, e o Kestrel nunca carrega um. A temperatura da placa de vídeo continua
no Live load da Home.

## Mudanças em outras telas do v4
- Home, painel "03 Rig": link "Open Hardware".
- Health, grupo "Hardware and BIOS": três linhas novas, `Graphics card uses all its PCIe lanes`,
  `PCIe link runs at full speed under load` e `Resizable BAR is on`. No grupo "Vanguard readiness":
  `Virtualization is on`.
- Toolbox (guia da BIOS): dois tópicos novos, `Run the graphics card at full PCIe speed` e
  `Turn on virtualization (only if you need it)`.
- Resultado do Benchmark: uma linha nova, `PCIe link under load: 4.0 x16`.

## Texto que o "Copy specs" copia
```
CPU: Intel Core i9-11900K (8 cores, 16 threads)
GPU: NVIDIA GeForce RTX 3060 Ti 8 GB · driver 610.74 · PCIe 4.0 x16
RAM: 32 GB DDR4-3600 · dual channel · 2 × 16 GB Corsair CMG32GX4M2D3600C18
Board: ASUS ROG MAXIMUS XIII HERO · BIOS 2103 (2024-05-20)
Displays: 1920×1080 at 360 Hz · 1920×1080 at 180 Hz
Storage: Samsung SSD 980 PRO 1TB (NVMe) · WDC WD20EZBX (HDD)
OS: Windows 11 Pro 25H2 (build 26200)
```

## O que devolver
- A tela Hardware em 1180×760 e em 1024×680, com os estados acima, no mesmo protótipo do v4 (ou num
  v5).
- Os dados de exemplo num bloco `hardware` do `mock-report.json`.
- As mudanças nas outras telas listadas acima.
