import { useMemo } from 'react';
import type { Board, Edge, PlayerColor, Port, Resource, Terrain, Tile, Vertex } from './types';

const VIEWBOX_PAD = 0.6;

interface BoardProps {
  board: Board;
  onTileClick?: (tileId: number) => void;
  onVertexClick?: (vertexId: number) => void;
  onEdgeClick?: (edgeId: number) => void;
  selectableTiles?: boolean;
  selectableVertices?: boolean;
  selectableEdges?: boolean;
  highlightTiles?: Set<number>;
  highlightVertices?: Set<number>;
  highlightEdges?: Set<number>;
  selectedEdges?: Set<number>;
}

export function Board({
  board,
  onTileClick,
  onVertexClick,
  onEdgeClick,
  selectableTiles,
  selectableVertices,
  selectableEdges,
  highlightTiles,
  highlightVertices,
  highlightEdges,
  selectedEdges,
}: BoardProps) {
  const viewBox = useMemo(() => {
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const v of board.vertices) {
      const { x, y } = v.position;
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }
    if (!Number.isFinite(minX)) return '-4 -4 8 8';
    const width = maxX - minX + 2 * VIEWBOX_PAD;
    const height = maxY - minY + 2 * VIEWBOX_PAD;
    return `${minX - VIEWBOX_PAD} ${minY - VIEWBOX_PAD} ${width} ${height}`;
  }, [board.vertices]);

  return (
    <svg
      viewBox={viewBox}
      style={{ width: '100%', height: '100%', maxHeight: '80vh', display: 'block' }}
      preserveAspectRatio="xMidYMid meet"
    >
      {board.tiles.map((tile) => (
        <TileShape
          key={tile.id}
          tile={tile}
          vertices={board.vertices}
          onClick={selectableTiles ? onTileClick : undefined}
          highlight={highlightTiles?.has(tile.id) ?? false}
        />
      ))}
      {board.edges.map((edge) => {
        const highlight = highlightEdges?.has(edge.id) ?? false;
        return (
          <EdgeShape
            key={edge.id}
            edge={edge}
            vertices={board.vertices}
            onClick={selectableEdges && highlight ? onEdgeClick : undefined}
            highlight={highlight}
            selected={selectedEdges?.has(edge.id) ?? false}
          />
        );
      })}
      {board.vertices.map((vertex) => {
        const highlight = highlightVertices?.has(vertex.id) ?? false;
        return (
          <VertexShape
            key={vertex.id}
            vertex={vertex}
            onClick={selectableVertices && highlight ? onVertexClick : undefined}
            highlight={highlight}
          />
        );
      })}
    </svg>
  );
}

function terrainClass(t: Terrain): string {
  const map: Record<Terrain, string> = {
    Hills: 'tile-hills',
    Forest: 'tile-forest',
    Mountains: 'tile-mountains',
    Fields: 'tile-fields',
    Pasture: 'tile-pasture',
    Desert: 'tile-desert',
  };
  return map[t];
}

function playerClass(c: PlayerColor): string {
  const map: Record<PlayerColor, string> = {
    Red: 'player-red',
    Blue: 'player-blue',
    White: 'player-white',
    Orange: 'player-orange',
  };
  return map[c];
}

function portLabel(port: Port): string {
  if (port === 'Generic3To1') return '3:1';
  return `2:1 ${resourceShort(port.Specific2To1)}`;
}

function resourceShort(r: Resource): string {
  const map: Record<Resource, string> = {
    Brick: 'B',
    Lumber: 'L',
    Ore: 'O',
    Grain: 'G',
    Wool: 'W',
  };
  return map[r];
}

function pointsFromVertices(tile: Tile, vertices: Vertex[]): string {
  return tile.vertices
    .map((vid) => {
      const v = vertices[vid];
      return `${v.position.x},${v.position.y}`;
    })
    .join(' ');
}

function tileCenter(tile: Tile, vertices: Vertex[]): { x: number; y: number } {
  const pts = tile.vertices.map((vid) => vertices[vid].position);
  const x = pts.reduce((s, p) => s + p.x, 0) / pts.length;
  const y = pts.reduce((s, p) => s + p.y, 0) / pts.length;
  return { x, y };
}

function getPlayerColor(id: number): PlayerColor {
  const colors: PlayerColor[] = ['Red', 'Blue', 'White', 'Orange'];
  return colors[id % colors.length];
}

interface TileShapeProps {
  tile: Tile;
  vertices: Vertex[];
  onClick?: (id: number) => void;
  highlight: boolean;
}

function TileShape({ tile, vertices, onClick, highlight }: TileShapeProps) {
  const center = tileCenter(tile, vertices);
  return (
    <g className={`tile-shape ${highlight ? 'tile-highlight' : ''} ${onClick ? 'tile-clickable' : ''}`}>
      <polygon
        className={`tile-base ${terrainClass(tile.terrain)}`}
        points={pointsFromVertices(tile, vertices)}
        stroke="#333"
        strokeWidth={0.05}
        onClick={() => onClick?.(tile.id)}
      />
      {tile.number !== null && (
        <g transform={`translate(${center.x}, ${center.y})`}>
          <circle className="number-disc" r={0.35} stroke="#333" strokeWidth={0.03} />
          <text
            className={`number-token ${tile.number === 6 || tile.number === 8 ? 'red' : ''}`}
            textAnchor="middle"
            dominantBaseline="middle"
            fontSize={0.35}
            pointerEvents="none"
          >
            {tile.number}
          </text>
        </g>
      )}
      {tile.has_robber && (
        <g className="robber" transform={`translate(${center.x}, ${center.y})`}>
          <circle r={0.2} stroke="#fff" strokeWidth={0.03} />
        </g>
      )}
    </g>
  );
}

interface EdgeShapeProps {
  edge: Edge;
  vertices: Vertex[];
  onClick?: (id: number) => void;
  highlight: boolean;
  selected: boolean;
}

function EdgeShape({ edge, vertices, onClick, highlight, selected }: EdgeShapeProps) {
  const a = vertices[edge.endpoints[0]].position;
  const b = vertices[edge.endpoints[1]].position;
  const hasRoad = edge.road !== null;
  const roadColorClass = hasRoad ? `road-${playerClass(getPlayerColor(edge.road!))}` : '';

  return (
    <g
      className={`edge-shape ${onClick ? 'edge-clickable' : ''} ${highlight ? 'edge-highlight' : ''} ${selected ? 'edge-selected' : ''}`}
      onClick={() => onClick?.(edge.id)}
      style={{ cursor: onClick ? 'pointer' : 'default' }}
    >
      {!hasRoad && <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} className="edge-base" />}
      {hasRoad && <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} className={`road ${roadColorClass}`} strokeWidth={0.15} />}
      {onClick && <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} className="edge-hit" />}
    </g>
  );
}

interface VertexShapeProps {
  vertex: Vertex;
  onClick?: (id: number) => void;
  highlight: boolean;
}

function VertexShape({ vertex, onClick, highlight }: VertexShapeProps) {
  const { x, y } = vertex.position;
  const building = vertex.building;
  const hasBuilding = building !== null;
  let ownerClass = '';
  let isCity = false;
  if (building) {
    if ('Settlement' in building) {
      ownerClass = playerClass(getPlayerColor(building.Settlement));
    } else {
      ownerClass = playerClass(getPlayerColor(building.City));
      isCity = true;
    }
  }

  return (
    <g
      className={`vertex-shape ${onClick ? 'vertex-clickable' : ''} ${highlight ? 'vertex-highlight' : ''}`}
      onClick={() => onClick?.(vertex.id)}
      style={{ cursor: onClick ? 'pointer' : 'default' }}
    >
      {highlight && !hasBuilding && <circle cx={x} cy={y} r={0.22} className="vertex-halo" />}
      {!hasBuilding && onClick && <circle cx={x} cy={y} r={0.18} className="vertex-dot" />}
      {hasBuilding && !isCity && (
        <circle cx={x} cy={y} r={0.18} className={`building settlement ${ownerClass}`} />
      )}
      {hasBuilding && isCity && (
        <g className={`building city ${ownerClass}`}>
          <rect x={x - 0.18} y={y - 0.18} width={0.36} height={0.36} />
          <rect x={x - 0.1} y={y - 0.3} width={0.2} height={0.15} />
        </g>
      )}
      {!hasBuilding && <circle cx={x} cy={y} r={0.06} className="vertex-empty" pointerEvents="none" />}
      {vertex.port && (
        <text className="port-label" x={x} y={y - 0.35} textAnchor="middle" fontSize={0.22} fontWeight="bold" pointerEvents="none">
          {portLabel(vertex.port)}
        </text>
      )}
    </g>
  );
}
