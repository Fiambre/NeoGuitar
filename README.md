<p align="center"><img src="assets/icon/icon-256.png" width="128" alt="NeoGuitar"></p>

# NeoGuitar

Lector, visor y reproductor de tablaturas **Guitar Pro** escrito en Rust, con interfaz en [egui](https://github.com/emilk/egui).

## Qué hace

- **Formatos:** GP3, GP4, GP5, GP6 (`.gpx`) y GP7/8 (`.gp`). El formato se detecta por el contenido del archivo.
- **Tablatura y pentagrama** (notación estándar con la fuente Bravura): ligaduras, plicas y barras, silencios, armadura, acordes con diagrama y letra.
- **Técnicas expresivas:** bends, barra de trémolo, slides, hammer-on/pull-off, vibrato, armónicos, trinos, notas de adorno, tremolo picking, notas muertas y ligadas.
- **Reproducción** con un SoundFont (`.sf2`): repeticiones y finales alternativos, cambios de tempo, swing, pitch bend, solo/silenciar y volumen por pista, metrónomo, cuenta regresiva, bucle de compases, velocidad ajustable y **entrenador de velocidad** (sube el tempo en cada vuelta del bucle).
- **Vista multipista:** varias pistas a la vez en el mismo sistema.
- **Mástil virtual:** muestra en el brazo de la guitarra las notas que suenan.
- **Secciones:** salto rápido a Intro, Verso, Coro… según los marcadores del archivo.
- **Exportación:** audio (WAV), MIDI y tablatura en texto.
- Ajustes persistentes (zoom, velocidad, archivos recientes, SoundFont…).

## Instalación

Descarga el instalador (`NeoGuitar-x.y.z-setup.exe`) o el zip portable desde [Releases](https://github.com/Fiambre/NeoGuitar/releases). El instalador no pide permisos de administrador (se puede elegir instalar para todos los usuarios) e incluye una casilla opcional para **asociar los archivos .gp3, .gp4, .gp5, .gpx y .gp con NeoGuitar**. Más adelante también se puede hacer desde la aplicación: **Más → Asociar archivos Guitar Pro**.

## Compilar y ejecutar

Requiere [Rust](https://rustup.rs) (edición 2024).

```
cargo run --release -- archivo.gp5
```

También puedes abrir un archivo desde la aplicación (Ctrl+O) o arrastrarlo a la ventana.

### SoundFont

El repositorio incluye [GeneralUser GS](https://schristiancollins.com/generaluser.php) en `soundfonts/` (licencia en `soundfonts/LICENSE-GeneralUser-GS.txt`), así que el audio funciona al compilar. NeoGuitar busca un `.sf2` en una carpeta `soundfonts/` junto al ejecutable o al directorio de trabajo, o en la variable `NEOGUITAR_SOUNDFONT`; también puedes elegir otro desde **Más → Elegir SoundFont…**.

### Atajos

| Tecla | Acción |
|---|---|
| `Espacio` | Reproducir / pausar |
| `Inicio` | Detener y volver al principio |
| `M` | Metrónomo |
| `Ctrl+O` | Abrir archivo |
| Clic derecho sobre un compás | Definir el bucle |

## Pruebas

```
cargo test --release
```

`samples/extra/` contiene archivos de prueba de [PyGuitarPro](https://github.com/Perlence/PyGuitarPro) y `tests/reference.rs` compara el resultado de la lectura con esa implementación de referencia (`tests/fixtures/dump_ref.py` regenera los datos).
Las tablaturas de canciones usadas en otras pruebas no se incluyen por sus derechos de autor; esos tests se omiten si faltan.

## Publicar una versión

Una etiqueta `vX.Y.Z` dispara el workflow *Release* (`.github/workflows/release.yml`): ejecuta las pruebas, compila, genera el instalador con [Inno Setup](https://jrsoftware.org/isinfo.php) (`installer/neoguitar.iss`) y el zip portable, y los adjunta a un release en GitHub. El workflow *CI* compila y prueba en cada push y pull request.

```
 git tag v0.1.0
 git push origin v0.1.0
```

## Licencia y créditos

NeoGuitar se distribuye bajo **LGPL-2.1 o posterior** (ver `LICENSE`).

- Los lectores de GP3–GP7 (`src/gp.rs`, `src/gpx.rs`) son una adaptación a Rust del código de [TuxGuitar](https://github.com/helge17/tuxguitar) (LGPL).
- Fuente de notación [Bravura](https://github.com/steinbergmedia/bravura) (SIL Open Font License, ver `assets/fonts/OFL.txt`).
- Archivos de prueba de [PyGuitarPro](https://github.com/Perlence/PyGuitarPro) (LGPL-3.0).
- SoundFont [GeneralUser GS](https://schristiancollins.com/generaluser.php) de S. Christian Collins (licencia propia, permite su uso y redistribución en proyectos de software).
- Síntesis con [rustysynth](https://github.com/sinshu/rustysynth), audio con [cpal](https://github.com/RustAudio/cpal).
