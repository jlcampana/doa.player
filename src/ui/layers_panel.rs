//! The "Capas" palette: the loaded costume's pieces, grouped by the node that moves them, each
//! with an eye, as in Photoshop. Alt+click on an eye shows only that piece (or group), and
//! again shows all. The selected piece has its opacity and whether its physics run.

use crate::app::{App, GameSource};
use crate::renderer::mesh::PieceLayer;
use std::collections::HashSet;

/// What the palette keeps between frames.
#[derive(Default)]
pub struct LayersUi {
    pub selected: Option<usize>,
    /// Nodes whose groups are folded.
    folded: HashSet<usize>,
}

/// A name for a node of DOA3's and DOA2U's skeleton, whose numbers mean the same bones.
/// Other games number their nodes by the file, so they keep the number.
fn node_name(source: GameSource, node: usize) -> String {
    if !matches!(source, GameSource::Doa2U | GameSource::Doa3) {
        return format!("Nodo {node}");
    }
    let name = match node {
        0 => "Pecho",
        1 => "Cabeza",
        2 => "Pelvis",
        3 | 9 => "Pie",
        4 | 10 => "Antebrazo",
        5 | 11 => "Mano",
        6 | 12 => "Muslo",
        7 | 13 => "Pierna",
        8 | 14 => "Brazo",
        19 | 20 => "Giro del antebrazo",
        23 | 24 => "Fin del giro",
        _ => "Nodo",
    };
    format!("{name} ({node})")
}

pub fn layers_panel(ui: &mut egui::Ui, app: &mut App, state: &mut LayersUi) {
    let pieces = app.mesh.piece_summaries();
    if pieces.is_empty() {
        ui.label(egui::RichText::new("Sin modelo cargado").color(egui::Color32::GRAY));
        return;
    }
    let mut layers: Vec<PieceLayer> = app.mesh.piece_layers().to_vec();
    let before = layers.clone();
    let source = app.char_panel.selected_source;
    ui.horizontal(|ui| {
        if ui.button("Mostrar todas").clicked() {
            layers.iter_mut().for_each(|l| l.visible = true);
        }
        if ui.button("Ocultar todas").clicked() {
            layers.iter_mut().for_each(|l| l.visible = false);
        }
    });
    let shown = layers.iter().filter(|l| l.visible).count();
    ui.weak(format!("{shown} de {} piezas visibles", layers.len()));

    // Groups in the order their nodes first appear.
    let mut nodes: Vec<usize> = Vec::new();
    for piece in &pieces {
        if !nodes.contains(&piece.node) {
            nodes.push(piece.node);
        }
    }
    let alt = ui.input(|i| i.modifiers.alt);
    // Alt+click: the pieces to leave alone on screen.
    let mut solo: Option<Vec<usize>> = None;
    let eye = |ui: &mut egui::Ui, visible: bool, partly: bool| {
        let text = if visible { "👁" } else if partly { "◐" } else { "  " };
        ui.add(egui::Button::new(text).min_size(egui::vec2(24.0, 0.0))).on_hover_text("Clic: mostrar u ocultar. Alt+clic: solo esta")
    };
    egui::ScrollArea::vertical().id_salt("layers").max_height((ui.available_height() - 150.0).max(160.0)).auto_shrink([false, true]).show(ui, |ui| {
        for &node in &nodes {
            let members: Vec<usize> = (0..pieces.len()).filter(|&i| pieces[i].node == node).collect();
            let all = members.iter().all(|&i| layers[i].visible);
            let any = members.iter().any(|&i| layers[i].visible);
            let folded = state.folded.contains(&node);
            ui.horizontal(|ui| {
                if eye(ui, all, any).clicked() {
                    if alt {
                        solo = Some(members.clone());
                    } else {
                        members.iter().for_each(|&i| layers[i].visible = !all);
                    }
                }
                if ui.small_button(if folded { "⏵" } else { "⏷" }).clicked() {
                    if folded {
                        state.folded.remove(&node);
                    } else {
                        state.folded.insert(node);
                    }
                }
                ui.label(egui::RichText::new(format!("{} · {}", node_name(source, node), members.len())).strong());
            });
            if folded {
                continue;
            }
            for &i in &members {
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    if eye(ui, layers[i].visible, false).clicked() {
                        if alt {
                            solo = Some(vec![i]);
                        } else {
                            layers[i].visible = !layers[i].visible;
                        }
                    }
                    let mut name = format!("Pieza {i}");
                    if layers[i].opacity < 1.0 {
                        name += &format!(" · {:.0} %", layers[i].opacity * 100.0);
                    }
                    if layers[i].frozen {
                        name += " · quieta";
                    }
                    let label = ui.selectable_label(state.selected == Some(i), name);
                    let label = label.on_hover_text(format!("{} vértices, {} triángulos", pieces[i].vertices, pieces[i].triangles));
                    if label.clicked() {
                        state.selected = Some(i);
                    }
                });
            }
        }
    });
    if let Some(keep) = solo {
        // As in Photoshop: Alt+click leaves only that one; another Alt+click on it shows all.
        let already = layers.iter().enumerate().all(|(i, l)| l.visible == keep.contains(&i));
        for (i, layer) in layers.iter_mut().enumerate() {
            layer.visible = already || keep.contains(&i);
        }
    }
    if let Some(i) = state.selected.filter(|&i| i < layers.len()) {
        ui.separator();
        ui.label(egui::RichText::new(format!("Pieza {i} · {}", node_name(source, pieces[i].node))).strong());
        ui.add(egui::Slider::new(&mut layers[i].opacity, 0.0..=1.0).text("Opacidad"));
        let mut physics = !layers[i].frozen;
        egui::ComboBox::from_id_salt("piece physics")
            .selected_text(if physics { "Física: como el juego" } else { "Física: quieta" })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut physics, true, "como el juego");
                ui.selectable_value(&mut physics, false, "quieta");
            })
            .response
            .on_hover_text("Quieta deja la pieza como se cargó: cadenas, tela, muelles, pecho y manos dejan de moverla, también en la exportación.");
        layers[i].frozen = !physics;
    } else {
        state.selected = None;
    }
    let queue = app.ctx.queue.clone();
    for (i, (layer, old)) in layers.into_iter().zip(before).enumerate() {
        if layer != old {
            app.mesh.set_piece_layer(&queue, i, layer);
        }
    }
}
