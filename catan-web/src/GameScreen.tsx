import { useMemo } from 'react';
import { Board } from './Board';
import { Controls } from './Controls';
import type { Pending } from './Controls';
import { GameLogPanel } from './GameLogPanel';
import type { Action, GameLog, GameView, PlayerView } from './types';
import {
  validCityVertices,
  validRoadEdges,
  validRobberTiles,
  validSettlementVertices,
  validSetupRoadEdges,
  validSetupSettlementVertices,
} from './build-helpers';

interface GameScreenProps {
  game: GameView;
  log: GameLog | null;
  send: (action: Action) => void;
  pending: Pending | null;
  setPending: (pending: Pending | null) => void;
  robberTile: number | null;
  setRobberTile: (tile: number | null) => void;
}

const colorMap: Record<string, string> = {
  Red: '#d32f2f',
  Blue: '#1976d2',
  White: '#eee',
  Orange: '#f57c00',
};

export function GameScreen({ game, log, send, pending, setPending, robberTile, setRobberTile }: GameScreenProps) {
  const me = game.players.find((p) => p.id === game.my_id)!;
  const current = game.players.find((p) => p.id === game.current_player)!;
  const phase = game.phase;
  const turnPhase = typeof phase === 'object' && 'Turn' in phase ? phase.Turn : null;
  const setupPhase = typeof phase === 'object' && 'Setup' in phase ? phase.Setup : null;
  const canResolveSeven = turnPhase === 'ResolveSeven' && Object.keys(game.pending_discards).length === 0;

  const validRoads = useMemo(() => validRoadEdges(game, me), [game, me]);
  const validSettlements = useMemo(() => validSettlementVertices(game, me), [game, me]);
  const validCities = useMemo(() => validCityVertices(game, me), [game, me]);
  const validSetupSettlements = useMemo(() => validSetupSettlementVertices(game), [game]);
  const validSetupRoads = useMemo(() => validSetupRoadEdges(game, me), [game, me]);
  const validTiles = useMemo(() => validRobberTiles(game), [game]);

  const handleTileClick = (tileId: number) => {
    if (pending?.kind === 'robber' || pending?.kind === 'knight') {
      setRobberTile(tileId);
      return;
    }
    if (canResolveSeven) {
      setPending({ kind: 'robber' });
      setRobberTile(tileId);
      return;
    }
  };

  const handleVertexClick = (vertexId: number) => {
    if (setupPhase && setupPhase.awaiting_road_for === null) {
      send({ PlaceSettlement: vertexId });
      return;
    }
    if (turnPhase === 'Main' && game.current_player === me.id) {
      const building = game.board.vertices[vertexId].building;
      if (building && 'Settlement' in building && building.Settlement === me.id) {
        send({ BuildCity: vertexId });
      } else {
        send({ BuildSettlement: vertexId });
      }
    }
  };

  const handleEdgeClick = (edgeId: number) => {
    if (setupPhase && setupPhase.awaiting_road_for !== null) {
      send({ PlaceRoad: edgeId });
      return;
    }
    if (pending?.kind === 'roadBuilding') {
      const edges = [...pending.edges, edgeId];
      if (edges.length === 2) {
        send({ PlayRoadBuilding: [edges[0], edges[1]] as [number, number] });
        setPending(null);
      } else {
        setPending({ kind: 'roadBuilding', edges });
      }
      return;
    }
    if (turnPhase === 'Main' && game.current_player === me.id) {
      send({ BuildRoad: edgeId });
    }
  };

  const chooseVictim = (target: number | null) => {
    if (robberTile === null) return;
    if (pending?.kind === 'robber') {
      send({ MoveRobber: { tile: robberTile, target } });
    } else if (pending?.kind === 'knight') {
      send({ PlayKnight: { tile: robberTile, target } });
    }
    setPending(null);
    setRobberTile(null);
  };

  const victims =
    robberTile !== null
      ? game.board.tiles[robberTile].vertices
          .map((vid) => game.board.vertices[vid].building)
          .filter((b): b is { Settlement: number } | { City: number } => b !== null)
          .map((b) => ('Settlement' in b ? b.Settlement : b.City))
          .filter((id, i, arr) =>
            arr.indexOf(id) === i && id !== me.id && (game.players.find((player) => player.id === id)?.resource_count ?? 0) > 0
          )
      : [];

  let vertexHighlight = new Set<number>();
  let edgeHighlight = new Set<number>();
  let tileHighlight = new Set<number>();

  if (game.current_player === me.id) {
    if (setupPhase && setupPhase.awaiting_road_for === null) {
      vertexHighlight = validSetupSettlements;
    } else if (setupPhase && setupPhase.awaiting_road_for !== null) {
      edgeHighlight = validSetupRoads;
    } else if (turnPhase === 'Main') {
      vertexHighlight = new Set([...validSettlements, ...validCities]);
      if (!pending || pending.kind === 'roadBuilding') {
        edgeHighlight = validRoads;
      }
    }
  }

  if (canResolveSeven || pending?.kind === 'robber' || pending?.kind === 'knight') {
    tileHighlight = validTiles;
  }

  const selectedEdges = new Set(pending?.kind === 'roadBuilding' ? pending.edges : []);

  return (
    <div className="app">
      <header>
        <h1>Catan</h1>
        <div className="players">
          {game.players.map((p) => (
            <PlayerBadge key={p.id} player={p} isCurrent={p.id === game.current_player} />
          ))}
        </div>
        {game.dice && (
          <div className="dice">
            Dice: {game.dice[0]} + {game.dice[1]} = {game.dice[0] + game.dice[1]}
          </div>
        )}
      </header>

      <div className={`status-bar${current.is_bot ? ' bot-turn' : ''}`}>
        {statusMessage(game, current, me)}
      </div>

      <div className="game-layout">
        <div className="board">
          <Board
            board={game.board}
            onTileClick={
              canResolveSeven || pending?.kind === 'robber' || pending?.kind === 'knight'
                ? handleTileClick
                : undefined
            }
            onVertexClick={handleVertexClick}
            onEdgeClick={handleEdgeClick}
            selectableTiles={
              canResolveSeven || pending?.kind === 'robber' || pending?.kind === 'knight'
            }
            selectableVertices
            selectableEdges
            highlightTiles={tileHighlight}
            highlightVertices={vertexHighlight}
            highlightEdges={edgeHighlight}
            selectedEdges={selectedEdges}
          />
        </div>
        <div className="panel">
          {robberTile !== null && (
            <div className="victim-picker">
              <strong>Choose a player to steal from:</strong>
              {victims.length === 0 && <button onClick={() => chooseVictim(null)}>None / skip</button>}
              {victims.map((pid) => (
                <button key={pid} onClick={() => chooseVictim(pid)}>
                  {game.players[pid]?.name}
                </button>
              ))}
            </div>
          )}
          <Controls game={game} me={me} send={send} setPending={setPending} pending={pending} />
          <GameLogPanel log={log} players={game.players} board={game.board} />
        </div>
      </div>
    </div>
  );
}

function playerLabel(player: PlayerView): string {
  return player.is_bot ? `${player.name} (bot)` : player.name;
}

function statusMessage(game: GameView, current: PlayerView, me: PlayerView): string {
  if (typeof game.phase === 'object' && 'Setup' in game.phase) {
    const setup = game.phase.Setup;
    const action = setup.awaiting_road_for === null ? 'placing a settlement' : 'placing a road';
    return `${playerLabel(current)} is ${action} (setup ${setup.round === 0 ? '1' : '2'}).`;
  }
  if (game.winner !== null) {
    const winner = game.players.find((p) => p.id === game.winner);
    return winner ? `${playerLabel(winner)} wins!` : 'Game over';
  }
  const base =
    game.current_player === me.id
      ? 'Your turn'
      : current.is_bot
        ? `${playerLabel(current)} is taking a turn`
        : `${current.name}'s turn`;
  const phase = typeof game.phase === 'object' && 'Turn' in game.phase ? ` — ${game.phase.Turn}` : '';
  return `${base}${phase}`;
}

function PlayerBadge({ player, isCurrent }: { player: PlayerView; isCurrent: boolean }) {
  return (
    <div
      className={`player-badge${isCurrent ? ' current' : ''}${player.is_bot ? ' is-bot' : ''}`}
      style={{ backgroundColor: colorMap[player.color] || '#888', color: player.color === 'White' ? '#111' : '#fff' }}
    >
      {player.name}
      {player.is_bot && <span className="bot-label">bot</span>}
      {' '}VP:{player.victory_points} RC:{player.resource_count}
    </div>
  );
}
