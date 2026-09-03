import type { Board, Building, Edge, GameView, PlayerView, Resources } from './types';

function otherVertex(edge: Edge, vertexId: number): number | null {
  const [a, b] = edge.endpoints;
  if (a === vertexId) return b;
  if (b === vertexId) return a;
  return null;
}

function adjacentVertexIds(board: Board, vertexId: number): number[] {
  const ids: number[] = [];
  for (const edgeId of board.vertices[vertexId].incident_edges) {
    const other = otherVertex(board.edges[edgeId], vertexId);
    if (other !== null) ids.push(other);
  }
  return ids;
}

export const ROAD_COST: Resources = { brick: 1, lumber: 1, ore: 0, grain: 0, wool: 0 };
export const SETTLEMENT_COST: Resources = { brick: 1, lumber: 1, ore: 0, grain: 1, wool: 1 };
export const CITY_COST: Resources = { brick: 0, lumber: 0, ore: 3, grain: 2, wool: 0 };
export const DEV_CARD_COST: Resources = { brick: 0, lumber: 0, ore: 1, grain: 1, wool: 1 };

function ownerOf(building: Building): number {
  if ('Settlement' in building) return building.Settlement;
  return building.City;
}

function isOpponentBuilding(building: Building | null, playerId: number): boolean {
  return building !== null && ownerOf(building) !== playerId;
}

function canAfford(resources: Resources | undefined, cost: Resources): boolean {
  if (!resources) return false;
  return (
    resources.brick >= cost.brick &&
    resources.lumber >= cost.lumber &&
    resources.ore >= cost.ore &&
    resources.grain >= cost.grain &&
    resources.wool >= cost.wool
  );
}

export function reachableVertices(board: Board, playerId: number): Set<number> {
  const visited = new Set<number>();
  const queue: number[] = [];

  for (const [id, vertex] of board.vertices.entries()) {
    if (vertex.building && ownerOf(vertex.building) === playerId) {
      visited.add(id);
      queue.push(id);
    }
  }

  while (queue.length) {
    const v = queue.pop()!;
    const vertex = board.vertices[v];
    for (const edgeId of vertex.incident_edges) {
      const edge = board.edges[edgeId];
      if (edge.road === playerId) {
        const other = otherVertex(edge, v);
        if (other !== null && !isOpponentBuilding(board.vertices[other].building, playerId) && !visited.has(other)) {
          visited.add(other);
          queue.push(other);
        }
      }
    }
  }

  return visited;
}

export function validRoadEdges(game: GameView, me: PlayerView): Set<number> {
  const ids = new Set<number>();
  if (!me.my_resources || me.remaining_roads === 0 || !canAfford(me.my_resources, ROAD_COST)) return ids;
  const board = game.board;
  const reach = reachableVertices(board, me.id);
  for (const edge of board.edges) {
    if (edge.road !== null) continue;
    const [a, b] = edge.endpoints;
    if (isOpponentBuilding(board.vertices[a].building, me.id)) continue;
    if (isOpponentBuilding(board.vertices[b].building, me.id)) continue;
    if (reach.has(a) || reach.has(b)) {
      ids.add(edge.id);
    }
  }
  return ids;
}

export function validSettlementVertices(game: GameView, me: PlayerView): Set<number> {
  const ids = new Set<number>();
  if (
    !me.my_resources ||
    me.remaining_settlements === 0 ||
    !canAfford(me.my_resources, SETTLEMENT_COST)
  )
    return ids;
  const board = game.board;
  for (const vertex of board.vertices) {
    if (vertex.building !== null) continue;
    const adjacent = adjacentVertexIds(board, vertex.id);
    if (adjacent.some((adj) => board.vertices[adj].building !== null)) continue;
    const connected = vertex.incident_edges.some((eid) => board.edges[eid].road === me.id);
    if (!connected) continue;
    ids.add(vertex.id);
  }
  return ids;
}

export function validCityVertices(game: GameView, me: PlayerView): Set<number> {
  const ids = new Set<number>();
  if (!me.my_resources || me.remaining_cities === 0 || !canAfford(me.my_resources, CITY_COST)) return ids;
  const board = game.board;
  for (const vertex of board.vertices) {
    const b = vertex.building;
    if (b && 'Settlement' in b && b.Settlement === me.id) {
      ids.add(vertex.id);
    }
  }
  return ids;
}

export function validSetupSettlementVertices(game: GameView): Set<number> {
  const ids = new Set<number>();
  const board = game.board;
  for (const vertex of board.vertices) {
    if (vertex.building !== null) continue;
    const adjacent = adjacentVertexIds(board, vertex.id);
    if (adjacent.some((adj) => board.vertices[adj].building !== null)) continue;
    ids.add(vertex.id);
  }
  return ids;
}

export function validSetupRoadEdges(game: GameView, me: PlayerView): Set<number> {
  const ids = new Set<number>();
  if (me.remaining_roads === 0) return ids;
  const phase = game.phase;
  if (typeof phase !== 'object' || !('Setup' in phase)) return ids;
  const settlement = phase.Setup.awaiting_road_for;
  if (settlement === null) return ids;
  for (const edge of game.board.edges) {
    if (edge.road !== null) continue;
    if (edge.endpoints.includes(settlement)) ids.add(edge.id);
  }
  return ids;
}

export function validRobberTiles(game: GameView): Set<number> {
  const ids = new Set<number>();
  const current = game.board.tiles.findIndex((t) => t.has_robber);
  for (const tile of game.board.tiles) {
    if (tile.id !== current) ids.add(tile.id);
  }
  return ids;
}

export {
  canAfford,
  isOpponentBuilding,
  ownerOf,
};
