<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../assets/banner-dark.svg">
  <img alt="diskgc — un limpiador de disco en segundo plano para equipos de desarrollo" src="../assets/banner-light.svg" width="100%">
</picture>

**Un limpiador de disco en segundo plano para equipos de desarrollo que sabe qué está borrando.**

[![CI](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml/badge.svg)](https://github.com/eukalpia/diskgc/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/eukalpia/diskgc?display_name=tag&sort=semver)](https://github.com/eukalpia/diskgc/releases)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](../../LICENSE)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-555)
![Rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-orange.svg)

[English](../../README.md) · [Русский](README.ru.md) · [O‘zbekcha](README.uz.md) · [简体中文](README.zh-CN.md) · **Español** · [हिन्दी](README.hi.md) · [العربية](README.ar.md) · [Français](README.fr.md) · [Português](README.pt-BR.md) · [日本語](README.ja.md)

<img alt="El panel de terminal de diskgc: resumen, un plan con un motivo para cada acción, informe e historial" src="../assets/dashboard.gif" width="100%">

</div>

## Por qué

Los discos de los desarrolladores se llenan de cosas que nadie pidió conservar: directorios `target/` de Cargo que crecen 100 GB en dos días, `node_modules` de ramas abandonadas en julio, cachés de compilación de Go que cada ejecución de pruebas vuelve a llenar, contenedores que dejó una prueba interrumpida, directorios temporales de agentes de programación con IA. macOS agrupa todo eso bajo "System Data" y no ofrece ayuda; `du` indica *dónde* está el espacio, pero no *qué es seguro borrar*.

diskgc se ejecuta como un pequeño daemon, vigila el espacio libre y su tendencia, y elimina solo lo que una herramienta puede regenerar. Para cada acción da un motivo escrito, te permite leer un plan antes de que pase nada y guarda un historial de lo que liberó.

## Destacados

- **Explica sus propias decisiones.** Cada acción incluye un motivo: *"el directorio `target/` ocupa 105 GB (> límite de 40 GB): todo lo que tenga más de un día"*, *"proyecto sin cambios durante 30 días; restaurado por install"*.
- **Conoce proyectos, no solo carpetas.** `target/` se identifica por su contenido, los directorios de compilación solo se consideran junto a su marcador de proyecto, y la actividad se evalúa sobre el proyecto completo (un git worktree cuenta como proyecto independiente).
- **Actúa antes de que el disco se llene.** Un pronóstico de las últimas seis horas adelanta la limpieza; los umbrales se endurecen de *normal* a *bajo* y luego a *crítico*.
- **Nunca toca lo que importa.** El código fuente, `.git`, las bases de datos y los historiales de chat nunca son candidatos; los globs excluidos nunca se tocan; los logs se comprimen en su lugar, no se eliminan.
- **Encuentra la causa.** `diskgc stats` muestra el crecimiento por día y por categoría, además de las rutas que vuelven una y otra vez: los lugares que conviene corregir en lugar de limpiar.
- **Panel de terminal y JSON.** Un TUI de seis pantallas, y `--json` en todos los comandos para scripts.

## Inicio rápido

```sh
cargo install --locked --git https://github.com/eukalpia/diskgc diskgc
diskgc config --edit      # define en [scan] roots las rutas donde están tus proyectos
diskgc plan               # qué eliminaría una limpieza y por qué — no se toca nada
diskgc install            # inicia el daemon al iniciar sesión (launchd / systemd --user / Programador de tareas)
diskgc status
```

Los binarios precompilados para macOS (Apple Silicon, Intel), Linux (x86_64) y Windows (x86_64) se adjuntan a cada [release](https://github.com/eukalpia/diskgc/releases): descomprime `diskgc` (y opcionalmente `diskgc-ui`) en cualquier directorio de tu `PATH`.

<div align="center">
<img alt="plan de diskgc en una terminal: acciones agrupadas con tamaños y motivos" src="../assets/plan.gif" width="100%">
</div>

## Qué limpia

| Regla | Qué | Cuándo |
|-------|-----|--------|
| Targets de Rust | directorios `target/` (encontrados por su contenido, estén donde estén) | artefactos sin tocar durante 7 días; por encima de 20 GB, también cachés incrementales y binarios de prueba pesados; por encima de 40 GB, todo lo que tenga más de un día; un presupuesto de 40 GB para todos los targets juntos |
| node_modules | `node_modules` junto a un `package.json` | nada del proyecto cambió durante 30 días |
| Directorios de compilación | `.dart_tool`, `build`, `_build`, `.next`, `.turbo`, `__pycache__`, `.gradle`, … junto a un marcador de proyecto | la misma regla de 30 días |
| Directorios temporales | área temporal por sesión de Claude Code, tus propias entradas en el directorio temporal del sistema, directorios de trabajo obsoletos en el `$TMPDIR` de cada usuario (macOS) | sin actividad durante 3 días / 7 días / 24 h |
| Herramientas de IA | versiones antiguas de la aplicación Codex, instalaciones de runtime abandonadas | siempre (conserva la actual + 1) |
| Docker | testcontainers huérfanos, contenedores detenidos, volúmenes e imágenes colgantes, caché de compilación | 2 h / 24 h / 7 d |
| Cachés de herramientas | cachés de Go, npm, uv, pnpm y sccache por encima de un límite, restos de actualizaciones de Chrome y VS Code, descargas de Homebrew, DerivedData de Xcode, cachés de simuladores y shaders de Metal | límites por regla |
| Archivos inactivos | logs de sesión e historiales JSONL de Codex y Claude Code | **comprimidos** por el sistema de archivos (APFS, NTFS), nunca eliminados |

## Cómo decide

El daemon se activa cada 10 minutos y revisa el espacio libre:

| Espacio libre | Comportamiento |
|---------------|----------------|
| por encima de `comfortable_gb` (150) | higiene programada cada 6 horas |
| por debajo de `comfortable_gb` | limpia ahora con las ventanas normales |
| por debajo de `low_gb` (100) | nivel *bajo*: todas las ventanas de antigüedad se reducen a la mitad |
| por debajo de `critical_gb` (60) | nivel *crítico*: ventanas reducidas a un cuarto, límites de tamaño a la mitad |

Un ajuste por mínimos cuadrados sobre las últimas seis horas de instantáneas predice el espacio libre dentro de 24 horas; si eso cruza un umbral, el daemon escala con antelación. Cada ejecución queda registrada, así que `diskgc stats` muestra el crecimiento por día y por categoría, además de los infractores recurrentes.

## Seguridad

diskgc borra archivos (esa es su función), por eso está diseñado para ser predecible:

- Solo propone lo que una herramienta puede regenerar (salidas de compilación, directorios de dependencias, cachés, archivos temporales) y da el motivo de cada acción; `plan` y `clean -n` muestran exactamente esa lista sin tocar nada.
- El código fuente, `.git`, las bases de datos y los historiales de chat nunca son candidatos; cualquier coincidencia con `[scan] exclude` nunca se toca; el ejecutor rechaza cualquier ruta fuera de `[safety] allowed_prefixes` (por defecto, tu directorio home y las carpetas del sistema para temporales y cachés).
- `[safety] delete_mode = "trash"` mueve los elementos a la papelera en lugar de borrarlos de forma permanente (macOS y Linux; si la papelera está en otro volumen, recurre al borrado).
- Cada ejecución queda registrada (`diskgc history`) con lo que se planificó y lo que realmente se liberó.

Sigue ejecutándose sin supervisión con tus permisos, así que revisa el primer `plan` en tu equipo antes de instalar el daemon.

## Panel de terminal

`diskgc ui` abre un panel construido con [Cinder](https://github.com/eukalpia/cinder) que se comunica con el daemon a través de su [protocolo IPC](../PROTOCOL.md):

| Pantalla | |
|----------|---|
| **Overview** | espacio libre, nivel, pronóstico, a dónde va el espacio |
| **Plan** | cada acción propuesta con su motivo: desmarca lo que debe quedarse, ejecuta el resto o haz una simulación |
| **Report** | los mayores infractores por categoría |
| **History** | ejecuciones, crecimiento por día, infractores recurrentes |
| **Memory** | RAM por programa, procesos auxiliares huérfanos |
| **Log** | actividad del daemon en vivo |

```
1-6 / tab   switch screens        r   refresh the current screen
p           build a plan          space / enter   toggle the highlighted action
x           execute selected      n   dry run             a   toggle all
k           terminate orphaned helper processes (Memory)   q   quit
```

El panel es un ejecutable nativo independiente (`diskgc-ui`); `diskgc ui` lo busca junto a `diskgc` o en `PATH` e inicia el daemon si no hay ninguno en ejecución.

## Comandos

```
diskgc report            qué ocupa el disco: categorías y los mayores infractores
diskgc plan              qué haría una limpieza, con motivos — no se toca nada
diskgc clean [-n]        limpia ahora (-n = solo plan)
diskgc status            estado del daemon, espacio libre, pronóstico, última ejecución
diskgc stats             crecimiento por día, tendencias por categoría, infractores recurrentes
diskgc history           ejecuciones recientes y lo que eliminaron
diskgc mem               RAM por proceso, procesos auxiliares huérfanos (--kill-orphans)
diskgc install | uninstall
diskgc config [--edit|--show]
diskgc ui                panel de terminal
```

Todos los comandos aceptan `--json`.

## Configuración

`~/.config/diskgc/config.toml` (Linux, macOS) o `%APPDATA%\diskgc\config.toml` (Windows), que se crea con valores por defecto en la primera ejecución y que el daemon vuelve a leer en cada ciclo. Cada clave está descrita en [docs/CONFIG.md](../CONFIG.md). Lo esencial:

```toml
[scan]
roots = ["~/work", "~/src"]          # dónde están tus proyectos
exclude = ["~/work/keep-forever/**"] # globs que ninguna regla puede tocar

[thresholds]
comfortable_gb = 150
low_gb = 100
critical_gb = 60

[safety]
delete_mode = "permanent"            # o "trash"
```

## Plataformas

| Plataforma | Estado |
|------------|--------|
| macOS (APFS) | uso diario; la plataforma principal |
| Linux | probado en CI y en contenedores; inicio automático con `systemd --user` |
| Windows | compila y pasa las pruebas en CI; el inicio automático con el Programador de tareas es la parte menos probada, así que se agradecen los reportes |

La compresión transparente está disponible en APFS y NTFS. Consulta [docs/ARCHITECTURE.md](../ARCHITECTURE.md) para ver la matriz de plataformas.

## Compilación desde el código fuente

```sh
cargo build --release
cargo test
cd ui && dart pub get && dart compile exe bin/diskgc_ui.dart -o build/diskgc-ui
```

Las grabaciones de este README se pueden reproducir a partir de un directorio home sintético: consulta [docs/demo](../demo).

## Contribuir

Se aceptan issues y pull requests; consulta [CONTRIBUTING.md](../../CONTRIBUTING.md). Una nueva regla necesita una prueba contra un árbol de directorios sintético que muestre tanto lo que elimina como lo que debe dejar intacto.

## Licencia

[Apache-2.0](../../LICENSE).

---

*Esta es una traducción del README original. La versión de referencia es el [README en inglés](../../README.md).*
