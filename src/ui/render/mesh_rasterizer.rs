//! Rasterize DrawCommand into GPU drawable mesh

use std::convert::TryFrom;

use crate::{
    color::Color,
    math::{vec2, vec3, Rect, RectOffset, Vec2},
    texture::Texture2D,
    ui::render::DrawCommand,
};

pub use crate::models::Vertex;

const MAX_VERTICES: usize = 8000;
const MAX_INDICES: usize = 4000;

#[derive(Debug)]
pub struct DrawList {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub clipping_zone: Option<Rect>,
    pub texture: Option<Texture2D>,
}

impl DrawList {
    pub const fn new() -> DrawList {
        DrawList {
            vertices: vec![],
            indices: vec![],
            clipping_zone: None,
            texture: None,
        }
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.clipping_zone = None;
    }

    pub fn draw_rectangle_lines(&mut self, rect: Rect, source: Rect, color: Color) {
        let Rect { x, y, w, h } = rect;

        self.draw_rectangle(Rect { x, y, w, h: 1. }, source, color);
        self.draw_rectangle(
            Rect {
                x: x + w - 1.,
                y: y + 1.,
                w: 1.,
                h: h - 2.,
            },
            source,
            color,
        );
        self.draw_rectangle(
            Rect {
                x,
                y: y + h - 1.,
                w,
                h: 1.,
            },
            source,
            color,
        );
        self.draw_rectangle(
            Rect {
                x,
                y: y + 1.,
                w: 1.,
                h: h - 2.,
            },
            source,
            color,
        );
    }

    fn draw_sprite(
        &mut self,
        rect: Rect,
        src: Rect,
        offsets: RectOffset,
        uv_offsets: RectOffset,
        color: Color,
    ) {
        let Rect { x, y, w, h } = rect;

        let RectOffset {
            left, right, top, ..
        } = offsets;

        let RectOffset {
            left: left0,
            right: right0,
            top: top0,
            bottom: bottom0,
        } = uv_offsets;

        let xs = [x, x + left, x + w - right, x + w];
        let ys = [y, y + top, y + h - top, y + h];

        let us = [src.x, src.x + left0, src.x + src.w - right0, src.x + src.w];
        let vs = [src.y, src.y + top0, src.y + src.h - bottom0, src.y + src.h];

        let mut n = 0;
        let mut vertices =
            [Vertex::new(0.0, 0.0, 0.0, 0.0, 0.0, Color::new(0.0, 0.0, 0.0, 0.0)); 16];

        for (x, u) in xs.iter().zip(us.iter()) {
            for (y, v) in ys.iter().zip(vs.iter()) {
                vertices[n].position = vec3(*x, *y, 0.);
                vertices[n].uv = vec2(*u, *v);
                vertices[n].color = color.into();
                n += 1;
            }
        }
        assert!(n == 16);

        let mut indices: [u16; 54] = [0; 54];
        n = 0;
        for row in 0..3 {
            for column in 0..3 {
                indices[n] = row * 4 + column;
                indices[n + 1] = row * 4 + column + 1;
                indices[n + 2] = (row + 1) * 4 + column;
                n += 3;
                indices[n] = row * 4 + column + 1;
                indices[n + 1] = (row + 1) * 4 + column;
                indices[n + 2] = (row + 1) * 4 + (column + 1);
                n += 3;
            }
        }
        assert!(n == 54);

        let indices_offset = self.vertices.len() as u16;
        self.vertices.extend_from_slice(&vertices[..]);
        self.indices
            .extend(indices.iter().map(|i| i + indices_offset));
    }

    fn draw_rectangle(&mut self, rect: Rect, src: Rect, color: Color) {
        let Rect { x, y, w, h } = rect;

        #[rustfmt::skip]
        let vertices = [
            Vertex::new(x    , y    , 0.0, src.x        , src.y        , color),
            Vertex::new(x + w, y    , 0.0, src.x + src.w, src.y        , color),
            Vertex::new(x + w, y + h, 0.0, src.x + src.w, src.y + src.h, color),
            Vertex::new(x    , y + h, 0.0, src.x        , src.y + src.h, color),
        ];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

        let indices_offset = self.vertices.len() as u16;
        self.vertices.extend_from_slice(&vertices[..]);
        self.indices
            .extend(indices.iter().map(|i| i + indices_offset));
    }

    fn draw_triangle(&mut self, p0: Vec2, p1: Vec2, p2: Vec2, source: Rect, color: Color) {
        let vertices = [
            Vertex::new(p0.x, p0.y, 0.0, source.x, source.y, color),
            Vertex::new(p1.x, p1.y, 0.0, source.x, source.y, color),
            Vertex::new(p2.x, p2.y, 0.0, source.x, source.y, color),
        ];
        let indices: [u16; 3] = [0, 1, 2];

        let indices_offset = self.vertices.len() as u16;
        self.vertices.extend_from_slice(&vertices[..]);
        self.indices
            .extend(indices.iter().map(|i| i + indices_offset));
    }

    pub fn draw_line(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        thickness: f32,
        source: Rect,
        color: Color,
    ) {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let nx = -dy; // https://stackoverflow.com/questions/1243614/how-do-i-calculate-the-normal-vector-of-a-line-segment
        let ny = dx;

        let tlen = (nx * nx + ny * ny).sqrt() / (thickness * 0.5);
        if tlen < f32::EPSILON {
            return;
        }
        let tx = nx / tlen;
        let ty = ny / tlen;

        let vertices = &[
            Vertex::new(x1 + tx, y1 + ty, 0.0, source.x, source.y, color),
            Vertex::new(x1 - tx, y1 - ty, 0.0, source.x, source.y, color),
            Vertex::new(x2 + tx, y2 + ty, 0.0, source.x, source.y, color),
            Vertex::new(x2 - tx, y2 - ty, 0.0, source.x, source.y, color),
        ];
        let indices = &[0, 1, 2, 2, 1, 3];

        let indices_offset = self.vertices.len() as u16;
        self.vertices.extend_from_slice(&vertices[..]);
        self.indices
            .extend(indices.iter().map(|i| i + indices_offset));
    }
}

fn get_active_draw_list<'a, 'b>(
    draw_lists: &'a mut Vec<DrawList>,
    command: &'b DrawCommand,
) -> &'a mut DrawList {
    if draw_lists.len() == 0 {
        draw_lists.push(DrawList::new());
    }

    let last = draw_lists.last().unwrap();
    match command {
        DrawCommand::Clip { rect, .. } => {
            if last.clipping_zone != *rect {
                draw_lists.push(DrawList::new());
            }
        }
        DrawCommand::DrawRawTexture { texture, .. } => {
            if !last
                .texture
                .as_ref()
                .map_or(false, |t| t.texture == texture.texture)
            {
                let clipping_zone = last.clipping_zone;

                draw_lists.push(DrawList {
                    texture: Some(texture.clone()),
                    clipping_zone,
                    ..DrawList::new()
                });
            }
        }
        DrawCommand::DrawCharacter { .. }
        | DrawCommand::DrawLine { .. }
        | DrawCommand::DrawRect { .. }
        | DrawCommand::DrawSprite { .. }
        | DrawCommand::DrawTriangle { .. } => {
            let (vertices, indices) = command.estimate_triangles_budget();

            if last.texture != None
                || last.vertices.len() + vertices >= MAX_VERTICES
                || last.indices.len() + indices >= MAX_INDICES
            {
                let clipping_zone = last.clipping_zone;

                draw_lists.push(DrawList {
                    clipping_zone,
                    ..DrawList::new()
                });
            }
        }
    }
    draw_lists.last_mut().unwrap()
}

pub(crate) fn render_command(draw_lists: &mut Vec<DrawList>, command: DrawCommand) {
    let active_draw_list = get_active_draw_list(draw_lists, &command);

    match command {
        DrawCommand::Clip { rect, .. } => {
            active_draw_list.clipping_zone = rect;
        }
        DrawCommand::DrawRect {
            rect,
            source,
            stroke,
            fill,
        } => {
            if let Some(fill) = fill {
                active_draw_list.draw_rectangle(rect, source, fill);
            }
            if let Some(stroke) = stroke {
                active_draw_list.draw_rectangle_lines(rect, source, stroke);
            }
        }
        DrawCommand::DrawSprite {
            rect,
            source,
            color,
            offsets,
            offsets_uv,
        } => {
            active_draw_list.draw_sprite(
                rect,
                source,
                offsets.unwrap_or_default(),
                offsets_uv.unwrap_or_default(),
                color,
            );
        }
        DrawCommand::DrawLine {
            start,
            end,
            source,
            color,
        } => {
            active_draw_list.draw_line(start.x, start.y, end.x, end.y, 1., source, color);
        }
        DrawCommand::DrawCharacter {
            dest,
            source,
            color,
        } => {
            active_draw_list.draw_rectangle(dest, source, color);
        }
        DrawCommand::DrawRawTexture { rect, .. } => {
            active_draw_list.draw_rectangle(
                rect,
                Rect::new(0., 0., 1., 1.),
                Color::new(1., 1., 1., 1.),
            );
        }
        DrawCommand::DrawTriangle {
            p0,
            p1,
            p2,
            source,
            color,
        } => {
            active_draw_list.draw_triangle(p0, p1, p2, source, color);
        }
    }
}

/// Split only built-in UI triangles; public mesh submissions retain their atomic contract.
pub(crate) fn submit_geometry_chunks(
    vertices: &[Vertex],
    indices: &[u16],
    max_v: usize,
    max_i: usize,
    mut submit: impl FnMut(&[Vertex], &[u16]),
) {
    if indices.is_empty() {
        return;
    }
    assert!(
        max_v >= 3 && max_i >= 3,
        "UI requires draw call capacities of at least 3 vertices and 3 indices"
    );
    assert_eq!(
        indices.len() % 3,
        0,
        "UI indices must contain complete triangles"
    );
    assert!(
        indices.iter().all(|&i| (i as usize) < vertices.len()),
        "UI index is outside its vertex slice"
    );
    let max_v = max_v.min(u16::MAX as usize + 1);
    let mut chunk_vertices = Vec::new();
    let mut chunk_indices = Vec::new();
    let mut remap = std::collections::HashMap::new();
    for triangle in indices.chunks_exact(3) {
        let added = triangle
            .iter()
            .enumerate()
            .filter(|&(n, i)| !remap.contains_key(i) && !triangle[..n].contains(i))
            .count();
        if chunk_vertices.len() + added > max_v || chunk_indices.len() + 3 > max_i {
            submit(&chunk_vertices, &chunk_indices);
            chunk_vertices.clear();
            chunk_indices.clear();
            remap.clear();
        }
        for &source in triangle {
            let index = *remap.entry(source).or_insert_with(|| {
                let index =
                    u16::try_from(chunk_vertices.len()).expect("UI chunk vertex index overflow");
                chunk_vertices.push(vertices[source as usize]);
                index
            });
            chunk_indices.push(index);
        }
    }
    if !chunk_indices.is_empty() {
        submit(&chunk_vertices, &chunk_indices);
    }
}

#[cfg(test)]
mod chunk_tests {
    use super::*;
    use glam::Vec4;

    #[test]
    fn chunks_preserve_triangle_order_and_every_vertex_attribute() {
        let vertices: Vec<_> = (0..8)
            .map(|n| {
                let n = n as f32;
                let mut v = Vertex::new(
                    n,
                    n + 1.,
                    n + 2.,
                    n / 8.,
                    n / 9.,
                    Color::new(n / 8., 0.5, 1., 1.),
                );
                v.normal = Vec4::new(n, 2. * n, -n, 3.);
                v
            })
            .collect();
        // Repeated/degenerate triangles are intentional, and must remain ordered.
        let indices = [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 0, 0, 1, 7, 2, 0];
        let expected: Vec<_> = indices
            .iter()
            .map(|&i| format!("{:?}", vertices[i as usize]))
            .collect();
        for (max_v, max_i) in [(3, 3), (4, 6), (8, 3), (3, 30), (8, 30)] {
            let mut actual = Vec::new();
            let mut calls = 0;
            submit_geometry_chunks(&vertices, &indices, max_v, max_i, |v, i| {
                assert!(v.len() <= max_v && i.len() <= max_i);
                assert_eq!(i.len() % 3, 0);
                actual.extend(i.iter().map(|&n| format!("{:?}", v[n as usize])));
                calls += 1;
            });
            assert_eq!(actual, expected);
            if (max_v, max_i) == (8, 30) {
                assert_eq!(calls, 1);
            }
        }
    }

    #[test]
    fn empty_and_invalid_chunks_never_submit_partial_work() {
        let v = [Vertex::new(0., 0., 0., 0., 0., Color::default()); 3];
        submit_geometry_chunks(&v, &[], 0, 0, |_, _| panic!("empty submission"));
        for (vertices, indices, max_v, max_i, message) in [
            (
                &v[..],
                &[0, 1, 2][..],
                2,
                3,
                "UI requires draw call capacities of at least 3 vertices and 3 indices",
            ),
            (
                &v[..],
                &[0, 1, 2][..],
                3,
                2,
                "UI requires draw call capacities of at least 3 vertices and 3 indices",
            ),
            (
                &v[..],
                &[0, 1, 2, 0, 1, 3][..],
                3,
                3,
                "UI index is outside its vertex slice",
            ),
            (
                &v[..],
                &[0, 1, 2, 0][..],
                3,
                3,
                "UI indices must contain complete triangles",
            ),
        ] {
            crate::test_backend::assert_panics_with(
                || {
                    submit_geometry_chunks(vertices, indices, max_v, max_i, |_, _| {
                        panic!("partial submission")
                    })
                },
                message,
            );
        }
    }
}
