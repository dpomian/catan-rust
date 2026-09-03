import type { Board, Edge, PlayerColor, Port, Resource, Terrain, Tile, Vertex } from './types';

interface BoardProps {
  board: Board;
  onTileClick?: (tileId: number) => void;
  onVertexClick?: (vertexId: number) => void;
  onEdgeClick?: (edgeId: number) => void;
  selectableTiles?: boolean;
  selectableVertices?: boolean;
  selectableEdges?: boolean;
}

export function Board({
  board,
  onTileClick,
  onVertexClick,
  onEdgeClick,
  selectableTiles,
  selectableVertices,
  selectableEdges,
}: BoardProps) {
  return (
    <svg
      viewBox="-4 -4 8 8"
      style={{ width: '100%', height: '100%', maxHeight: '80vh' }}
      preserveAspectRatio="xMidYMid meet"
    >
      {board.tiles.map((tile) => (
        <TileShape
          key={tile.id}
          tile={tile}
          vertices={board.vertices}
          onClick={selectableTiles ? onTileClick : undefined}
        />
      ))}
      {board.edges.map((edge) => (
        <EdgeShape
          key={edge.id}
          edge={edge}
          vertices={board.vertices}
          onClick={selectableEdges ? onEdgeClick : undefined}
        />
      ))}
      {board.vertices.map((vertex) => (
        <VertexShape
          key={vertex.id}
          vertex={vertex}
          onClick={selectableVertices ? onVertexClick : undefined}
        />
      ))}
    </svg>
  );
}

function terrainColor(t: Terrain): string {
  switch (t) {
    case 'Hills':
      return '#e6b84d';
    case 'Forest':
      return '#4a8f4a';
    case 'Mountains':
      return '#8c8c8c';
    case 'Fields':
      return '#f2d65a';
    case 'Pasture':
      return '#7fbf7f';
    case 'Desert':
      return '#e6d7a3';
  }
}

function playerColor(c: PlayerColor): string {
  switch (c) {
    case 'Red':
      return '#d32f2f';
    case 'Blue':
      return '#1976d2';
    case 'White':
      return '#f5f5f5';
    case 'Orange':
      return '#f57c00';
  }
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

interface TileShapeProps {
  tile: Tile;
  vertices: Vertex[];
  onClick?: (id: number) => void;
}

function TileShape({ tile, vertices, onClick }: TileShapeProps) {
  const center = tileCenter(tile, vertices);
  return (
    <g>
      <polygon
        points={pointsFromVertices(tile, vertices)}
        fill={terrainColor(tile.terrain)}
        stroke="#333"
        strokeWidth={0.05}
        onClick={() => onClick?.(tile.id)}
        style={{ cursor: onClick ? 'pointer' : 'default' }}
      />
      {tile.number !== null && (
        <g transform={`translate(${center.x}, ${center.y})`}>
          <circle r={0.35} fill="#f5f0e1" stroke="#333" strokeWidth={0.03} />
          <text
            textAnchor="middle"
            dominantBaseline="middle"
            fontSize={0.35}
            fill={tile.number === 6 || tile.number === 8 ? '#c00' : '#333'}
            pointerEvents="none"
          >
            {tile.number}
          </text>
        </g>
      )}
      {tile.has_robber && (
        <g transform={`translate(${center.x}, ${center.y})`}>
          <circle r={0.2} fill="#111" stroke="#fff" strokeWidth={0.03} />
        </g>
      )}
    </g>
  );
}

interface EdgeShapeProps {
  edge: Edge;
  vertices: Vertex[];
  onClick?: (id: number) => void;
}

function EdgeShape({ edge, vertices, onClick }: EdgeShapeProps) {
  const a = vertices[edge.endpoints[0]].position;
  const b = vertices[edge.endpoints[1]].position;
  const hasRoad = edge.road !== null;
  const color = hasRoad ? playerColor(getPlayerColor(edge.road!)) : 'transparent';

  return (
    <g onClick={() => onClick?.(edge.id)} style={{ cursor: onClick ? 'pointer' : 'default' }}>
      <line
        x1={a.x}
        y1={a.y}
        x2={b.x}
        y2={b.y}
        stroke={hasRoad ? color : 'transparent'}
        strokeWidth={hasRoad ? 0.15 : 0.25}
        strokeLinecap="round"
        opacity={hasRoad ? 1 : 0.3}
      />
      {!hasRoad && onClick && (
        <line
          x1={a.x}
          y1={a.y}
          x2={b.x}
          y2={b.y}
          stroke="transparent"
          strokeWidth={0.3}
        />
      )}
    </g>
  );
}

function getPlayerColor(id: number): PlayerColor {
  const colors: PlayerColor[] = ['Red', 'Blue', 'White', 'Orange'];
  return colors[id % colors.length];
}

interface VertexShapeProps {
  vertex: Vertex;
  onClick?: (id: number) => void;
}

function VertexShape({ vertex, onClick }: VertexShapeProps) {
  const { x, y } = vertex.position;
  const building = vertex.building;
  const hasBuilding = building !== null;
  let color = '#333';
  let isCity = false;
  if (building) {
    if ('Settlement' in building) {
      color = playerColor(getPlayerColor(building.Settlement));
    } else {
      color = playerColor(getPlayerColor(building.City));
      isCity = true;
    }
  }

  return (
    <g
      onClick={() => onClick?.(vertex.id)}
      style={{ cursor: onClick ? 'pointer' : 'default' }}
    >
      {!hasBuilding && onClick && (
        <circle cx={x} cy={y} r={0.18} fill="rgba(255,255,255,0.3)" stroke="none" />
      )}
      {hasBuilding && !isCity && (
        <circle cx={x} cy={y} r={0.18} fill={color} stroke="#111" strokeWidth={0.04} />
      )}
      {hasBuilding && isCity && (
        <>
          <rect
            x={x - 0.18}
            y={y - 0.18}
            width={0.36}
            height={0.36}
            fill={color}
            stroke="#111"
            strokeWidth={0.04}
          />
          <rect
            x={x - 0.1}
            y={y - 0.3}
            width={0.2}
            height={0.15}
            fill={color}
            stroke="#111"
            strokeWidth={0.04}
          />
        </>
      )}
      {!hasBuilding && (
        <circle cx={x} cy={y} r={0.08} fill="#222" opacity={0.4} pointerEvents="none" />
      )}
      {vertex.port && (
        <text
          x={x}
          y={y - 0.35}
          textAnchor="middle"
          fontSize={0.22}
          fill="#111"
          fontWeight="bold"
          pointerEvents="none"
        >
          {portLabel(vertex.port)}
        </text>
      )}
    </g>
  );
}
