use crate::types::*;
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub id: TileId,
    pub coord: CubeCoord,
    pub terrain: Terrain,
    pub number: Option<u8>,
    pub has_robber: bool,
    pub vertices: [VertexId; 6],
    pub edges: [EdgeId; 6],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub id: VertexId,
    pub position: Point,
    pub incident_tiles: Vec<TileId>,
    pub incident_edges: Vec<EdgeId>,
    pub port: Option<Port>,
    pub building: Option<Building>,
}

impl Vertex {
    pub fn is_coastal(&self) -> bool {
        self.incident_tiles.len() < 3
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub endpoints: [VertexId; 2],
    pub incident_tiles: Vec<TileId>,
    pub road: Option<PlayerId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub tiles: Vec<Tile>,
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Edge>,
}

/// Standard number token sequence used for variable setup (letters A–R counter-clockwise).
const NUMBER_TOKENS: [u8; 18] = [5, 2, 6, 3, 8, 10, 9, 12, 11, 4, 8, 10, 9, 4, 5, 6, 3, 11];

/// Port sequence placed around the coast. Each port covers two consecutive
/// coastal vertices.
const PORTS: [Port; 9] = [
    Port::Generic3To1,
    Port::Generic3To1,
    Port::Generic3To1,
    Port::Generic3To1,
    Port::Specific2To1(Resource::Wool),
    Port::Specific2To1(Resource::Lumber),
    Port::Specific2To1(Resource::Brick),
    Port::Specific2To1(Resource::Grain),
    Port::Specific2To1(Resource::Ore),
];

/// Terrain counts for the base Catan board.
const TERRAINS: [Terrain; 19] = [
    Terrain::Forest,
    Terrain::Forest,
    Terrain::Forest,
    Terrain::Forest,
    Terrain::Hills,
    Terrain::Hills,
    Terrain::Hills,
    Terrain::Pasture,
    Terrain::Pasture,
    Terrain::Pasture,
    Terrain::Pasture,
    Terrain::Fields,
    Terrain::Fields,
    Terrain::Fields,
    Terrain::Fields,
    Terrain::Mountains,
    Terrain::Mountains,
    Terrain::Mountains,
    Terrain::Desert,
];

impl Board {
    /// Create a standard randomized Catan board.
    pub fn new_standard() -> Self {
        let mut board = Self::generate_geometry();
        board.assign_terrain_and_numbers();
        board
    }

    /// Build the shared geometry for a standard Catan board (tiles, vertices, edges).
    fn generate_geometry() -> Self {
        let radius = 2;
        let size = 1.0;
        let sqrt3 = 3.0_f64.sqrt();

        // Corner offsets for a pointy-top hex with screen y-axis pointing down.
        // Order: top, top-right, bottom-right, bottom, bottom-left, top-left.
        let corner_offsets: [(f64, f64); 6] = [
            (0.0, -size),
            (sqrt3 * 0.5, -size * 0.5),
            (sqrt3 * 0.5, size * 0.5),
            (0.0, size),
            (-sqrt3 * 0.5, size * 0.5),
            (-sqrt3 * 0.5, -size * 0.5),
        ];

        let hex_coords: Vec<CubeCoord> = all_hexes_within_radius(radius);

        let mut tiles = Vec::with_capacity(hex_coords.len());
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut edges: Vec<Edge> = Vec::new();
        let mut vertex_map: HashMap<String, VertexId> = HashMap::new();
        let mut edge_map: HashMap<(VertexId, VertexId), EdgeId> = HashMap::new();

        for coord in &hex_coords {
            // Cube (x, y, z) with z as axial r and x as axial q.
            let q = coord.x;
            let r = coord.z;
            let center_x = size * sqrt3 * (q as f64 + r as f64 * 0.5);
            let center_y = size * 1.5 * r as f64;

            let mut tile_vertices = [0; 6];
            for (i, (dx, dy)) in corner_offsets.iter().enumerate() {
                let px = center_x + dx;
                let py = center_y + dy;
                let key = format!("{:.6},{:.6}", px, py);

                let vertex_id = *vertex_map.entry(key).or_insert_with(|| {
                    let id = vertices.len();
                    vertices.push(Vertex {
                        id,
                        position: Point { x: px, y: py },
                        incident_tiles: Vec::new(),
                        incident_edges: Vec::new(),
                        port: None,
                        building: None,
                    });
                    id
                });

                vertices[vertex_id].incident_tiles.push(tiles.len());
                tile_vertices[i] = vertex_id;
            }

            let mut tile_edges = [0; 6];
            for i in 0..6 {
                let v0 = tile_vertices[i];
                let v1 = tile_vertices[(i + 1) % 6];
                let key = if v0 < v1 { (v0, v1) } else { (v1, v0) };

                let edge_id = *edge_map.entry(key).or_insert_with(|| {
                    let id = edges.len();
                    edges.push(Edge {
                        id,
                        endpoints: [key.0, key.1],
                        incident_tiles: Vec::new(),
                        road: None,
                    });
                    id
                });

                edges[edge_id].incident_tiles.push(tiles.len());
                tile_edges[i] = edge_id;
            }

            // Add each edge to its endpoints' incident edge lists.
            for i in 0..6 {
                let edge_id = tile_edges[i];
                let v0 = tile_vertices[i];
                let v1 = tile_vertices[(i + 1) % 6];
                if !vertices[v0].incident_edges.contains(&edge_id) {
                    vertices[v0].incident_edges.push(edge_id);
                }
                if !vertices[v1].incident_edges.contains(&edge_id) {
                    vertices[v1].incident_edges.push(edge_id);
                }
            }

            tiles.push(Tile {
                id: tiles.len(),
                coord: *coord,
                terrain: Terrain::Desert,
                number: None,
                has_robber: false,
                vertices: tile_vertices,
                edges: tile_edges,
            });
        }

        let mut board = Board {
            tiles,
            vertices,
            edges,
        };

        board.assign_ports();
        board
    }

    /// Randomly assign terrains and number tokens, leaving the desert empty.
    fn assign_terrain_and_numbers(&mut self) {
        let mut rng = thread_rng();

        let mut terrains = TERRAINS.to_vec();
        terrains.shuffle(&mut rng);

        for (tile, terrain) in self.tiles.iter_mut().zip(terrains) {
            tile.terrain = terrain;
            if terrain == Terrain::Desert {
                tile.has_robber = true;
            }
        }

        let spiral = self.spiral_order();
        let mut token_iter = NUMBER_TOKENS.iter();

        for tile_id in spiral {
            let tile = &mut self.tiles[tile_id];
            if tile.terrain == Terrain::Desert {
                tile.number = None;
                continue;
            }
            if let Some(&number) = token_iter.next() {
                tile.number = Some(number);
            }
        }
    }

    /// Place the 9 ports on 18 selected coastal vertices, leaving gaps between them.
    fn assign_ports(&mut self) {
        let mut coastal: Vec<VertexId> = self
            .vertices
            .iter()
            .filter(|v| v.is_coastal())
            .map(|v| v.id)
            .collect();

        // Sort by angle around the board center.
        coastal.sort_by(|a, b| {
            let va = &self.vertices[*a];
            let vb = &self.vertices[*b];
            let angle_a = f64::atan2(va.position.y, va.position.x);
            let angle_b = f64::atan2(vb.position.y, vb.position.x);
            angle_a.partial_cmp(&angle_b).unwrap()
        });

        // Gaps (in vertices) between the 9 port pairs on the 30-vertex coastline.
        // Each pair covers 2 adjacent vertices, so gaps sum to 30 - 18 = 12.
        let gaps = [1, 2, 1, 1, 2, 1, 1, 2, 1];
        let mut start = 0;
        for (i, &gap) in gaps.iter().enumerate() {
            let port = PORTS[i];
            if let Some(v0) = coastal.get(start) {
                self.vertices[*v0].port = Some(port);
            }
            if let Some(v1) = coastal.get(start + 1) {
                self.vertices[*v1].port = Some(port);
            }
            start += 2 + gap;
        }
        debug_assert_eq!(start, coastal.len());
    }

    /// Return tile ids in a counter-clockwise spiral for number-token placement.
    fn spiral_order(&self) -> Vec<TileId> {
        let radius = 2;
        let mut order = Vec::with_capacity(self.tiles.len());

        for r in (0..=radius).rev() {
            let ring = ring_coords(r);
            for coord in ring {
                if let Some(tile) = self.tile_at(coord) {
                    order.push(tile.id);
                }
            }
        }

        order
    }

    /// Look up a tile by its cube coordinate.
    pub fn tile_at(&self, coord: CubeCoord) -> Option<&Tile> {
        self.tiles.iter().find(|t| t.coord == coord)
    }

    /// Mutable variant of `tile_at`.
    pub fn tile_at_mut(&mut self, coord: CubeCoord) -> Option<&mut Tile> {
        self.tiles.iter_mut().find(|t| t.coord == coord)
    }

    /// Return the vertices connected to this one by an edge.
    pub fn adjacent_vertices(&self, vertex_id: VertexId) -> Vec<VertexId> {
        let mut result = Vec::new();
        for edge_id in &self.vertices[vertex_id].incident_edges {
            let edge = &self.edges[*edge_id];
            for &v in &edge.endpoints {
                if v != vertex_id && !result.contains(&v) {
                    result.push(v);
                }
            }
        }
        result
    }

    /// Find the edge connecting two vertices, if any.
    pub fn edge_between(&self, v0: VertexId, v1: VertexId) -> Option<EdgeId> {
        let key = if v0 < v1 { (v0, v1) } else { (v1, v0) };
        self.edges
            .iter()
            .find(|e| e.endpoints == [key.0, key.1] || e.endpoints == [key.1, key.0])
            .map(|e| e.id)
    }

    /// Check whether a road has been built on this edge.
    pub fn is_edge_occupied(&self, edge_id: EdgeId) -> bool {
        self.edges[edge_id].is_occupied()
    }
}

impl Edge {
    /// Check whether a road has been built on this edge.
    pub fn is_occupied(&self) -> bool {
        self.road.is_some()
    }

    /// Return the opposite endpoint of this edge, if the vertex belongs to it.
    pub fn other_vertex(&self, vertex_id: VertexId) -> Option<VertexId> {
        if self.endpoints[0] == vertex_id {
            Some(self.endpoints[1])
        } else if self.endpoints[1] == vertex_id {
            Some(self.endpoints[0])
        } else {
            None
        }
    }
}

/// Return all cube coordinates within the given hex radius, including the center.
fn all_hexes_within_radius(radius: i32) -> Vec<CubeCoord> {
    let mut coords = Vec::new();
    for x in -radius..=radius {
        for y in -radius..=radius {
            let z = -x - y;
            if z.abs() <= radius {
                coords.push(CubeCoord::new(x, y, z));
            }
        }
    }
    coords
}

/// Return the coordinates of the hex ring at the given radius.
fn ring_coords(radius: i32) -> Vec<CubeCoord> {
    if radius == 0 {
        return vec![CubeCoord::new(0, 0, 0)];
    }

    // Start at the top-right corner for radius r: (r, 0, -r).
    let start = CubeCoord::new(radius, 0, -radius);

    // Side directions in counter-clockwise order (screen y down).
    let directions = [
        CubeCoord::new(-1, 1, 0), // top: right to left
        CubeCoord::new(-1, 0, 1), // top-left to bottom-left
        CubeCoord::new(0, -1, 1), // bottom-left to bottom
        CubeCoord::new(1, -1, 0), // bottom to bottom-right
        CubeCoord::new(1, 0, -1), // bottom-right to top-right
        CubeCoord::new(0, 1, -1), // top-right up
    ];

    let mut ring = vec![start];
    let mut current = start;
    for dir in &directions {
        for _ in 0..radius {
            current = CubeCoord::new(current.x + dir.x, current.y + dir.y, current.z + dir.z);
            ring.push(current);
        }
    }

    // The last step returns to the start; remove the duplicate.
    ring.pop();
    ring
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn board_has_19_tiles() {
        let board = Board::new_standard();
        assert_eq!(board.tiles.len(), 19);
    }

    #[test]
    fn board_has_54_vertices_and_72_edges() {
        let board = Board::new_standard();
        assert_eq!(board.vertices.len(), 54);
        assert_eq!(board.edges.len(), 72);
    }

    #[test]
    fn desert_has_robber_and_no_number() {
        let board = Board::new_standard();
        let desert = board
            .tiles
            .iter()
            .find(|t| t.terrain == Terrain::Desert)
            .unwrap();
        assert!(desert.has_robber);
        assert_eq!(desert.number, None);
    }

    #[test]
    fn terrain_counts_match_standard_board() {
        let board = Board::new_standard();
        let mut counts: HashMap<Terrain, usize> = HashMap::new();
        for tile in &board.tiles {
            *counts.entry(tile.terrain).or_default() += 1;
        }
        assert_eq!(counts.get(&Terrain::Forest), Some(&4));
        assert_eq!(counts.get(&Terrain::Hills), Some(&3));
        assert_eq!(counts.get(&Terrain::Pasture), Some(&4));
        assert_eq!(counts.get(&Terrain::Fields), Some(&4));
        assert_eq!(counts.get(&Terrain::Mountains), Some(&3));
        assert_eq!(counts.get(&Terrain::Desert), Some(&1));
    }

    #[test]
    fn non_desert_numbers_match_token_pool() {
        let board = Board::new_standard();
        let mut numbers: Vec<u8> = board
            .tiles
            .iter()
            .filter(|t| t.terrain != Terrain::Desert)
            .filter_map(|t| t.number)
            .collect();
        numbers.sort();
        let mut tokens = NUMBER_TOKENS.to_vec();
        tokens.sort();
        assert_eq!(numbers, tokens);
        assert!(numbers.iter().all(|&n| n != 7));
    }

    #[test]
    fn number_tokens_exclude_seven() {
        assert!(NUMBER_TOKENS.iter().all(|&n| n != 7));
    }

    #[test]
    fn tile_at_and_tile_at_mut() {
        let mut board = Board::generate_geometry();
        let coord = CubeCoord::new(0, 0, 0);
        assert!(board.tile_at(coord).is_some());
        board.tile_at_mut(coord).unwrap().terrain = Terrain::Hills;
        assert_eq!(board.tile_at(coord).unwrap().terrain, Terrain::Hills);
        assert!(board.tile_at(CubeCoord::new(10, -10, 0)).is_none());
    }

    #[test]
    fn spiral_order_contains_every_tile_once() {
        let board = Board::generate_geometry();
        let order = board.spiral_order();
        assert_eq!(order.len(), board.tiles.len());
        let unique: HashSet<_> = order.iter().copied().collect();
        assert_eq!(unique.len(), order.len());
    }

    #[test]
    fn all_hexes_within_radius_gives_19() {
        let coords = all_hexes_within_radius(2);
        assert_eq!(coords.len(), 19);
        for c in &coords {
            assert_eq!(c.x + c.y + c.z, 0);
        }
    }

    #[test]
    fn ring_coords_length_is_six_times_radius() {
        assert_eq!(ring_coords(0).len(), 1);
        assert_eq!(ring_coords(1).len(), 6);
        assert_eq!(ring_coords(2).len(), 12);
    }

    #[test]
    fn geometry_has_expected_incident_counts() {
        let board = Board::generate_geometry();
        for tile in &board.tiles {
            assert_eq!(tile.vertices.len(), 6);
            assert_eq!(tile.edges.len(), 6);
        }
        for edge in &board.edges {
            assert!(edge.incident_tiles.len() >= 1);
            assert!(edge.incident_tiles.len() <= 2);
        }
        for vertex in &board.vertices {
            assert!(vertex.incident_tiles.len() <= 3);
        }
    }

    #[test]
    fn edge_incident_tile_totals() {
        let board = Board::generate_geometry();
        let boundary = board
            .edges
            .iter()
            .filter(|e| e.incident_tiles.len() == 1)
            .count();
        let interior = board
            .edges
            .iter()
            .filter(|e| e.incident_tiles.len() == 2)
            .count();
        assert_eq!(boundary, 30);
        assert_eq!(interior, 42);
        assert_eq!(boundary + interior, board.edges.len());
    }

    #[test]
    fn adjacency_is_symmetric() {
        let board = Board::generate_geometry();
        for edge in &board.edges {
            let a = edge.endpoints[0];
            let b = edge.endpoints[1];
            assert!(board.adjacent_vertices(a).contains(&b));
            assert!(board.adjacent_vertices(b).contains(&a));
        }
    }

    #[test]
    fn edge_between_round_trips() {
        let board = Board::generate_geometry();
        for edge in &board.edges {
            let id = board
                .edge_between(edge.endpoints[0], edge.endpoints[1])
                .unwrap();
            assert_eq!(id, edge.id);
            assert!(
                board
                    .edge_between(edge.endpoints[0], edge.endpoints[0])
                    .is_none()
            );
        }
    }

    #[test]
    fn edge_other_vertex() {
        let board = Board::generate_geometry();
        let edge = &board.edges[0];
        let a = edge.endpoints[0];
        let b = edge.endpoints[1];
        assert_eq!(edge.other_vertex(a), Some(b));
        assert_eq!(edge.other_vertex(b), Some(a));
        assert_eq!(edge.other_vertex(9999), None);
    }

    #[test]
    fn is_edge_occupied_false_initially() {
        let board = Board::generate_geometry();
        assert!(!board.is_edge_occupied(0));
    }

    #[test]
    fn coastal_vertices_have_ports() {
        let board = Board::new_standard();
        let coastal: Vec<_> = board.vertices.iter().filter(|v| v.is_coastal()).collect();
        // The radius-2 board has 30 coastal vertices; the 9 ports cover 18 of them.
        assert_eq!(coastal.len(), 30);
        let assigned = coastal.iter().filter(|v| v.port.is_some()).count();
        assert_eq!(assigned, 18);
    }

    #[test]
    fn port_distribution_matches_definition() {
        let board = Board::new_standard();
        let mut generic = 0;
        let mut specific: HashMap<Resource, usize> = HashMap::new();
        for v in &board.vertices {
            match v.port {
                Some(Port::Generic3To1) => generic += 1,
                Some(Port::Specific2To1(r)) => {
                    *specific.entry(r).or_default() += 1;
                }
                None => {}
            }
        }
        // 18 port vertices = 4 generic ports * 2 + 5 specific ports * 2.
        assert_eq!(generic, 8);
        for r in Resource::all() {
            assert_eq!(specific.get(&r).copied().unwrap_or(0), 2);
        }
    }

    #[test]
    fn ports_are_only_on_coastal_vertices() {
        let board = Board::new_standard();
        for v in &board.vertices {
            if v.port.is_some() {
                assert!(v.is_coastal());
            }
        }
    }
}
