<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../assets/banner-dark.svg">
  <img alt="diskgc — um limpador de disco em segundo plano para máquinas de desenvolvedor" src="../assets/banner-light.svg" width="100%">
</picture>

**Um limpador de disco em segundo plano para máquinas de desenvolvedor que sabe o que está apagando.**

[![CI](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml/badge.svg)](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/eukalpia/diskgc?display_name=tag&sort=semver)](https://github.com/eukalpia/diskgc/releases)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../../LICENSE)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-555)
![Rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange.svg)

[English](../../README.md) · [Русский](README.ru.md) · [O‘zbekcha](README.uz.md) · [简体中文](README.zh-CN.md) · [Español](README.es.md) · [हिन्दी](README.hi.md) · [العربية](README.ar.md) · [Français](README.fr.md) · **Português** · [日本語](README.ja.md)

<img alt="O dashboard de terminal do diskgc: visão geral, um plano com um motivo para cada ação, relatório e histórico" src="../assets/dashboard.gif" width="100%">

</div>

## Por que

Discos de desenvolvedor enchem com coisas que ninguém pediu para guardar: diretórios `target/` do Cargo que crescem 100 GB em dois dias, `node_modules` de branches abandonadas em julho, caches de build do Go reabastecidos a cada execução de testes, containers que um teste interrompido deixou para trás, diretórios temporários de agentes de IA para programação. O macOS chama tudo isso de "Dados do Sistema" e não ajuda em nada; o `du` mostra *onde*, mas não *o que é seguro apagar*.

O diskgc roda como um pequeno daemon, acompanha o espaço livre e sua tendência, e remove apenas o que uma ferramenta consegue regenerar — com um motivo escrito para cada ação, um plano que você pode ler antes de qualquer coisa acontecer e um histórico do que foi liberado.

## Destaques

- **Explica a si mesmo.** Toda ação vem com um motivo: *"target is 105 GB (> 40 GB cap): everything older than a day"*, *"project untouched for 30 days; restored by install"*.
- **Conhece projetos, não só pastas.** O `target/` é encontrado pelo conteúdo, diretórios de build apenas ao lado do seu marcador de projeto, e a atividade é avaliada no projeto inteiro (um git worktree conta como projeto próprio).
- **Age antes que o disco fique cheio.** Uma previsão com as últimas seis horas antecipa a limpeza; os limites endurecem de *normal* para *low* e depois para *critical*.
- **Nunca toca no que importa.** Código-fonte, `.git`, bancos de dados e históricos de chat nunca são candidatos; globs excluídos nunca são tocados; logs são comprimidos no lugar, não apagados.
- **Encontra a causa.** O `diskgc stats` mostra o crescimento por dia e por categoria e os caminhos que continuam voltando — os lugares para corrigir em vez de apenas limpar.
- **Dashboard de terminal e JSON.** Uma TUI de seis telas, e `--json` em todos os comandos para scripts.

## Início rápido

```sh
cargo install --locked --git https://github.com/eukalpia/diskgc diskgc
diskgc config --edit      # defina [scan] roots para onde ficam seus projetos
diskgc plan               # o que uma limpeza removeria, e por quê — nada é tocado
diskgc install            # inicia o daemon no login (launchd / systemd --user / Task Scheduler)
diskgc status
```

Binários prontos para macOS (Apple Silicon, Intel), Linux (x86_64) e Windows (x86_64) estão anexados a cada [release](https://github.com/eukalpia/diskgc/releases): descompacte `diskgc` (e, se quiser, `diskgc-ui`) em algum lugar do seu `PATH`.

<div align="center">
<img alt="diskgc plan no terminal: ações agrupadas com tamanhos e motivos" src="../assets/plan.gif" width="100%">
</div>

## O que ele limpa

| Regra | O quê | Quando |
|------|------|------|
| Targets do Rust | diretórios `target/` (encontrados pelo conteúdo, onde quer que estejam) | artefatos sem uso há 7 dias; acima de 20 GB, também caches incrementais e binários de teste pesados; acima de 40 GB, tudo com mais de um dia; um orçamento de 40 GB para todos os targets juntos |
| node_modules | `node_modules` ao lado de um `package.json` | nada no projeto mudou há 30 dias |
| Diretórios de build | `.dart_tool`, `build`, `_build`, `.next`, `.turbo`, `__pycache__`, `.gradle`, … ao lado de um marcador de projeto | a mesma regra de 30 dias |
| Diretórios temporários | Scratch por sessão do Claude Code, suas próprias entradas no diretório temporário do sistema, diretórios de trabalho antigos no `$TMPDIR` do usuário (macOS) | sem atividade por 3 dias / 7 dias / 24 h |
| Ferramentas de IA | versões antigas do aplicativo Codex, instalações de runtime abandonadas | sempre (mantém a atual + 1) |
| Docker | testcontainers órfãos, containers parados, volumes e imagens sem tag (dangling), cache de build | 2 h / 24 h / 7 d |
| Caches de ferramentas | caches de Go, npm, uv, pnpm e sccache acima de um limite, sobras de atualização do Chrome e do VS Code, downloads do Homebrew, DerivedData do Xcode, caches de simulador e de shaders do Metal | limites por regra |
| Arquivos frios | logs de sessão e históricos JSONL do Codex e do Claude Code | **comprimidos** pelo sistema de arquivos (APFS, NTFS), nunca apagados |

## Como ele decide

O daemon acorda a cada 10 minutos e verifica o espaço livre:

| Espaço livre | Comportamento |
|------------|-----------|
| acima de `comfortable_gb` (150) | higienização agendada a cada 6 horas |
| abaixo de `comfortable_gb` | limpa agora, com as janelas normais |
| abaixo de `low_gb` (100) | nível *low*: todas as janelas de antiguidade são reduzidas pela metade |
| abaixo de `critical_gb` (60) | nível *critical*: janelas reduzidas a um quarto, limites de tamanho pela metade |

Um ajuste por mínimos quadrados sobre os snapshots das últimas seis horas prevê o espaço livre em 24 horas; se isso cruzar um limite, o daemon escala antecipadamente. Cada execução é registrada, então o `diskgc stats` mostra o crescimento por dia, por categoria e os principais reincidentes.

## Segurança

O diskgc apaga arquivos — essa é a função dele —, por isso foi construído para ser previsível:

- Ele só propõe o que uma ferramenta consegue regenerar (saída de build, diretórios de dependências, caches, arquivos temporários) e informa o motivo de cada ação; `plan` e `clean -n` mostram exatamente essa lista sem tocar em nada.
- Código-fonte, `.git`, bancos de dados e históricos de chat nunca são candidatos; tudo que corresponde a `[scan] exclude` nunca é tocado; o executor recusa qualquer caminho fora de `[safety] allowed_prefixes` (por padrão, seu diretório home e as pastas temporárias e de cache do sistema).
- `[safety] delete_mode = "trash"` move os itens para a lixeira em vez de apagá-los de forma permanente (macOS e Linux; quando a lixeira está em outro volume, ele volta a apagar).
- Toda execução é registrada (`diskgc history`) com o que foi planejado e o que foi realmente liberado.

Ele continua rodando sem supervisão com as suas permissões, então leia o primeiro `plan` na sua máquina antes de instalar o daemon.

## Dashboard de terminal

`diskgc ui` abre um dashboard feito com o [Cinder](https://github.com/eukalpia/cinder) que conversa com o daemon pelo seu [protocolo IPC](../PROTOCOL.md):

| Tela | |
|--------|---|
| **Overview** | espaço livre, nível, previsão, para onde o espaço está indo |
| **Plan** | cada ação proposta com o seu motivo — desmarque o que deve ficar, execute o resto ou faça uma simulação (dry run) |
| **Report** | os maiores responsáveis por categoria |
| **History** | execuções, crescimento por dia, reincidentes |
| **Memory** | RAM por programa, processos auxiliares órfãos |
| **Log** | atividade ao vivo do daemon |

```
1-6 / tab   switch screens        r   refresh the current screen
p           build a plan          space / enter   toggle the highlighted action
x           execute selected      n   dry run             a   toggle all
k           terminate orphaned helper processes (Memory)   q   quit
```

O dashboard é um executável nativo separado (`diskgc-ui`); o `diskgc ui` o encontra ao lado do `diskgc` ou no `PATH` e inicia o daemon caso nenhum esteja rodando.

## Comandos

```
diskgc report            o que ocupa o disco: categorias e os maiores responsáveis
diskgc plan              o que uma limpeza faria, com motivos — nada é tocado
diskgc clean [-n]        limpa agora (-n = somente plano)
diskgc status            estado do daemon, espaço livre, previsão, última execução
diskgc stats             crescimento por dia, tendências por categoria, reincidentes
diskgc history           execuções recentes e o que cada uma removeu
diskgc mem               RAM por processo, processos auxiliares órfãos (--kill-orphans)
diskgc install | uninstall
diskgc config [--edit|--show]
diskgc ui                dashboard de terminal
```

Todos os comandos aceitam `--json`.

## Configuração

`~/.config/diskgc/config.toml` (Linux, macOS) ou `%APPDATA%\diskgc\config.toml` (Windows), criado com os valores padrão na primeira execução e relido pelo daemon a cada ciclo. Cada chave está descrita em [docs/CONFIG.md](../CONFIG.md). O essencial:

```toml
[scan]
roots = ["~/work", "~/src"]          # onde ficam seus projetos
exclude = ["~/work/keep-forever/**"] # globs que nenhuma regra pode tocar

[thresholds]
comfortable_gb = 150
low_gb = 100
critical_gb = 60

[safety]
delete_mode = "permanent"            # ou "trash"
```

## Plataformas

| Plataforma | Status |
|----------|--------|
| macOS (APFS) | usado diariamente; o principal alvo |
| Linux | testado no CI e em containers; autostart com `systemd --user` |
| Windows | compila e passa nos testes no CI; o autostart pelo Task Scheduler é a parte menos exercitada — relatos são bem-vindos |

A compressão transparente está disponível no APFS e no NTFS. Veja [docs/ARCHITECTURE.md](../ARCHITECTURE.md) para a matriz de plataformas.

## Compilando a partir do código-fonte

```sh
cargo build --release
cargo test
cd ui && dart pub get && dart compile exe bin/diskgc_ui.dart -o build/diskgc-ui
```

As gravações deste README são reproduzíveis a partir de um diretório home sintético: veja [docs/demo](../demo).

## Contribuindo

Issues e pull requests são bem-vindos — veja [CONTRIBUTING.md](../../CONTRIBUTING.md). Uma nova regra precisa de um teste contra uma árvore de diretórios sintética mostrando tanto o que ela remove quanto o que ela deve deixar intacto.

## Licença

[Apache-2.0](../../LICENSE).

---

*Esta é uma tradução do README. A referência é o README em inglês: [README.md](../../README.md).*
