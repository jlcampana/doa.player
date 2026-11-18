# Lo que depende de ti

Cosas que no puedo hacer solo: pruebas con el juego en marcha, decisiones y comprobaciones a ojo
contra el juego. Cuando hagas una, dímelo y la tacho aquí.

## Pruebas con el juego en marcha

Las herramientas leen la memoria del juego sin escribir nada.

### DOA2U
- [ ] **`ryu00` en combate** (Ryu Hayabusa con su traje `ryu00`; en el menú el primero es
  `ryu06`). Sirve para el cuerpo blando de tipo 14 (un objeto que apunta a un punto del cuerpo).
- [ ] **Cuándo se activa la cadena 9 de Helena** (y su objeto, tipo 8): solo vive en algunas fases
  del asalto. Hay que tenerla en pantalla mientras capturo.
- [ ] **Manos fotograma a fotograma** contra los guiones de movimiento: un luchador haciendo
  movimientos mientras capturo.
- [ ] **El abrigo de Helena al ser lanzada** (último error de las telas): Helena `hel00` en
  entrenamiento contra cualquiera que la lance, mientras capturo con
  `tools/re/doa2u_cloth_pairs.py` (ahora guarda también el byte de congelación del juego por
  fotograma). Con eso sabré si el juego congela la tela en esos fotogramas.
- [ ] **Los hombros de `kas00`** (hasta 2,6 cm): Kasumi `kas00` en pantalla, quieta y moviéndose,
  para una captura de vértices nueva.
- [ ] **Las tiras de la cabeza (`lei00`, `lei04`) y la capa de `tin09`**: solo encajan con 1 par por
  hoja; una captura nueva con Lei Fang y Tina en pantalla lo confirmaría.
- [ ] **Comparar la iluminación** con capturas del juego en `zac01`, `tin09` y `lei03`, cuando
  esté hecha la iluminación que falta.

### DOA3
- [ ] **Iluminación de DOA3**: una sesión con solo DOA3 abierto (tienes un mando).

### DOAX2
- [ ] **Rebote del pecho**: poner a Tina, Lisa o Helena en pantalla y capturar con
  `tools/re/doax2_live.py array target/captures/doax2_live.txt`; luego paso la prueba
  `live_spring_bones_match`.
- [ ] **Colores de los materiales y luces**: cómo los combina el juego (necesito el juego en
  marcha para leer las constantes).

### DOA4
- [ ] **El juego no arrancó en tu equipo el 2026-11-11.** Si llega a arrancar, todo lo de DOA4 se
  podría comparar con él (hasta ahora nada lo está).

### DOAX3 Fortune
- [ ] **El port no arranca**: necesita la actualización 1.19 y tres módulos del sistema de PS4
  (`libSceFiber.sprx`, `ulobjmgr.sprx`, `libSceUlt.sprx`). Si los consigues, se podría comparar
  todo DOAX3 con el juego (movimientos, física, materiales).

## Comprobaciones a ojo

- [ ] **DOAX3: ¿se ven bien los peinados ahora?** Seco por defecto; el mojado en la fila
  «Variant». ¿Recuerdas cuándo sale el pelo mojado en el juego?
- [ ] **DOAX3: física del pelo y de los cordones** (casilla «Physics»). Es una imitación mía, no la
  del juego: ¿te convence el movimiento, o lo prefieres más rígido o más suelto?
- [ ] **DOAX3: rebote del pecho**. También es un sustituto mío (control «Bounce»).
- [ ] **DOAX3: expresiones de la cara** (fila «Face»): salen numeradas; si reconoces alguna, puedo
  ponerle nombre.
- [ ] **DOAX2 y DOA4: el damero del pelo** quedó arreglado ocultando las caras traseras. Si ves
  algún trozo de traje o pelo que ahora desaparezca visto por dentro o por detrás, avísame.
- [ ] **Sombreado anime**: cuando lo trabaje, decirme si va hacia el estilo Fire Emblem que
  buscas (sería útil una captura de referencia de lo que te gusta).

## Decisiones

- [ ] **Exportación de DOAX3**: ¿quieres que el `.glb` lleve también la física del pelo y el rebote
  del pecho horneados fotograma a fotograma (ficheros más grandes), como ya pasa en DOA3 y DOA2U
  con un solo movimiento?
- [ ] **Ficheros sin seguimiento en el repositorio**: `continuar.sh` y los `__pycache__` de
  `tools/re/`. ¿Los añado al `.gitignore`, los comito o los dejo así?
