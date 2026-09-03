import { useState } from 'react';
import { Board } from './Board';
import { Controls } from './Controls';
import type { Pending } from './Controls';
import { useGame } from './useGame';
import type { Action, PlayerView } from './types';

function App() {
  const { connected, error, lobby, game, send } = useGame();
  const [name, setName] = useState('');
  const [joinCode, setJoinCode] = useState('');
  const [pending, setPending] = useState<Pending | null>(null);
  const [robberTile, setRobberTile] = useState<number | null>(null);

  const sendAction = (action: Action) => {
    send({ type: 'Action', action });
  };

  const createGame = () => {
    if (name.trim()) send({ type: 'CreateGame', name: name.trim() });
  };

  const joinGame = () => {
    if (name.trim() && joinCode.trim()) {
      send({ type: 'JoinGame', code: joinCode.trim().toUpperCase(), name: name.trim() });
    }
  };

  const startGame = () => {
    send({ type: 'StartGame' });
  };

  if (!connected && !game && !lobby) {
    return <div className="app">Connecting...</div>;
  }

  if (lobby) {
    return (
      <div className="app">
        <h1>Catan Lobby</h1>
        <p>Room code: <strong>{lobby.code}</strong></p>
        <ul>
          {lobby.players.map((p) => (
            <li key={p.id}>
              {p.name} {p.id === lobby.you ? '(you)' : ''} {p.id === lobby.host ? '(host)' : ''}
            </li>
          ))}
        </ul>
        {lobby.you === lobby.host && lobby.players.length >= 2 && (
          <button onClick={startGame}>Start Game</button>
        )}
        {lobby.you === lobby.host && lobby.players.length < 2 && (
          <p>Waiting for more players...</p>
        )}
      </div>
    );
  }

  if (game) {
    const me = game.players.find((p) => p.id === game.my_id)!;
    const phase = game.phase;
    const turnPhase = typeof phase === 'object' && 'Turn' in phase ? phase.Turn : null;
    const setupPhase = typeof phase === 'object' && 'Setup' in phase ? phase.Setup : null;

    const handleTileClick = (tileId: number) => {
      if (pending?.kind === 'robber' || pending?.kind === 'knight') {
        setRobberTile(tileId);
        return;
      }
      if (turnPhase === 'ResolveSeven') {
        setPending({ kind: 'robber' });
        setRobberTile(tileId);
        return;
      }
    };

    const handleVertexClick = (vertexId: number) => {
      if (setupPhase && setupPhase.awaiting_road_for === null) {
        sendAction({ PlaceSettlement: vertexId });
        return;
      }
      if (turnPhase === 'Main' && game.current_player === me.id) {
        const building = game.board.vertices[vertexId].building;
        if (building && 'Settlement' in building && building.Settlement === me.id) {
          sendAction({ BuildCity: vertexId });
        } else {
          sendAction({ BuildSettlement: vertexId });
        }
      }
    };

    const handleEdgeClick = (edgeId: number) => {
      if (setupPhase && setupPhase.awaiting_road_for !== null) {
        sendAction({ PlaceRoad: edgeId });
        return;
      }
      if (pending?.kind === 'roadBuilding') {
        const edges = [...pending.edges, edgeId];
        if (edges.length === 2) {
          sendAction({ PlayRoadBuilding: [edges[0], edges[1]] as [number, number] });
          setPending(null);
        } else {
          setPending({ kind: 'roadBuilding', edges });
        }
        return;
      }
      if (turnPhase === 'Main' && game.current_player === me.id) {
        sendAction({ BuildRoad: edgeId });
      }
    };

    const chooseVictim = (target: number | null) => {
      if (robberTile === null) return;
      if (pending?.kind === 'robber') {
        sendAction({ MoveRobber: { tile: robberTile, target } });
      } else if (pending?.kind === 'knight') {
        sendAction({ PlayKnight: { tile: robberTile, target } });
      }
      setPending(null);
      setRobberTile(null);
    };

    const victims = robberTile !== null
      ? game.board.tiles[robberTile].vertices
          .map((vid) => game.board.vertices[vid].building)
          .filter((b): b is { Settlement: number } | { City: number } => b !== null)
          .map((b) => ('Settlement' in b ? b.Settlement : b.City))
          .filter((id, i, arr) => arr.indexOf(id) === i && id !== me.id)
      : [];

    return (
      <div className="app">
        <header>
          <h1>Catan</h1>
          <div className="players">
            {game.players.map((p) => (
              <PlayerBadge key={p.id} player={p} isCurrent={p.id === game.current_player} />
            ))}
          </div>
          {game.dice && <div className="dice">Dice: {game.dice[0]} + {game.dice[1]} = {game.dice[0] + game.dice[1]}</div>}
        </header>

        <div className="game-layout">
          <div className="board">
            <Board
              board={game.board}
              onTileClick={turnPhase === 'ResolveSeven' || pending?.kind === 'robber' || pending?.kind === 'knight' ? handleTileClick : undefined}
              onVertexClick={handleVertexClick}
              onEdgeClick={handleEdgeClick}
              selectableTiles={turnPhase === 'ResolveSeven' || pending?.kind === 'robber' || pending?.kind === 'knight'}
              selectableVertices
              selectableEdges
            />
          </div>
          <div className="panel">
            {robberTile !== null && (
              <div className="victim-picker">
                <strong>Choose a player to steal from:</strong>
                <button onClick={() => chooseVictim(null)}>None / skip</button>
                {victims.map((pid) => (
                  <button key={pid} onClick={() => chooseVictim(pid)}>
                    {game.players[pid]?.name}
                  </button>
                ))}
              </div>
            )}
            <Controls game={game} me={me} send={sendAction} setPending={setPending} pending={pending} />
          </div>
        </div>

        {error && <div className="error">{error}</div>}
      </div>
    );
  }

  return (
    <div className="app">
      <h1>Catan</h1>
      {error && <div className="error">{error}</div>}
      <div className="lobby-form">
        <input
          placeholder="Your name"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <button onClick={createGame}>Create Game</button>
        <input
          placeholder="Room code"
          value={joinCode}
          onChange={(e) => setJoinCode(e.target.value)}
        />
        <button onClick={joinGame}>Join Game</button>
      </div>
    </div>
  );
}

function PlayerBadge({ player, isCurrent }: { player: PlayerView; isCurrent: boolean }) {
  const colorMap: Record<string, string> = {
    Red: '#d32f2f',
    Blue: '#1976d2',
    White: '#eee',
    Orange: '#f57c00',
  };
  return (
    <div
      className={`player-badge ${isCurrent ? 'current' : ''}`}
      style={{ backgroundColor: colorMap[player.color] || '#888', color: player.color === 'White' ? '#111' : '#fff' }}
    >
      {player.name} VP:{player.victory_points} RC:{player.resource_count}
    </div>
  );
}

export default App;
