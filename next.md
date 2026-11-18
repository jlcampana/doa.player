# Orden de trabajo

Decidido por el usuario el 2026-11-18. Se sigue en este orden; cada punto se cierra (o se deja
documentado lo que no se pueda hacer sin el juego) antes de pasar al siguiente.

1. ~~**Rebote del pecho en tres ejes**~~ Hecho el 2026-11-18 (ver punto 4 del backlog en
   `CLAUDE.md`): los ejes lateral y frontal son un sustituto del visor salvo en DOAX2.
   **Rebote del pecho en tres ejes** (vertical, lateral y adelante-atrás), en el visor y en la
   exportación a Blender, para todos los juegos. Hoy solo DOAX2 tiene los tres ejes (leídos de su
   código); DOA2U, DOA3, DOAX, DOA4 y DOAX3 tienen solo el vertical o un sustituto.
2. ~~**Sombreado anime**~~ Hecho el 2026-11-18 (ver "Development notes" en `CLAUDE.md`); falta
   tu opinión (`humano.md`).
   **Sombreado anime** (`cel_shaded`, `vs_outline` en `src/renderer/mesh.rs`): trabajarlo más, en
   la línea de Fire Emblem que pidió el usuario.
3. **DOA3, punto 2 del backlog**: el modo 1 de los tubos (`sub_00044620` y cuatro auxiliares; hay
   una captura en `target/captures/doa3_hitomi_tubes.bin`), y después lo demás de ese punto:
   otros objetivos de la cara, viento real del escenario, normales de los vértices, secciones
   0xFA, defectos visuales (bandas del antebrazo de Tina, objeto de la barbilla de Zack).
4. **DOA2U, punto 3 de "Next steps"**: el error que queda en los cuerpos blandos (abrigo de
   Helena al ser lanzada o golpeada, hombros hasta 2,6 cm, tiras de la cabeza y capa de `tin09`).
5. **DOAX, punto 1**: formas intermedias de las manos, giro del brazo, luces por escenario.
6. **Todo lo que se pueda**, por este orden de juegos:
   1. DOA2U
   2. DOA3
   3. DOA4
   4. DOAX2

Lo que depende del usuario (decisiones, pruebas con el juego en marcha) está en `humano.md`.
