import { useEffect, useState } from 'react';
import { GameScreen } from './GameScreen';
import { useGame } from './useGame';
import type { Action } from './types';
import type { Pending } from './Controls';
import './App.css';

const colorMap: Record<string, string> = {
  Red: '#d32f2f',
  Blue: '#1976d2',
  White: '#eee',
  Orange: '#f57c00',
};

function App() {
  const { connected, error, lobby, game, log, send, clearError } = useGame();
  const [name, setName] = useState('');
  const [joinCode, setJoinCode] = useState('');
  const [pending, setPending] = useState<Pending | null>(null);
  const [robberTile, setRobberTile] = useState<number | null>(null);

  useEffect(() => {
    if (!error) return;
    const timer = setTimeout(clearError, 5000);
    return () => clearTimeout(timer);
  }, [error, clearError]);

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

  const copyCode = async (code: string) => {
    try {
      await navigator.clipboard.writeText(code);
    } catch {
      // ignore
    }
  };

  if (!connected && !game && !lobby) {
    return <div className="app">Connecting...</div>;
  }

  if (lobby) {
    return (
      <div className="app">
        <h1>Catan Lobby</h1>
        <div className="lobby-code">
          Room code: <code>{lobby.code}</code>
          <button onClick={() => copyCode(lobby.code)}>Copy</button>
        </div>
        <ul className="lobby-list">
          {lobby.players.map((p) => (
            <li key={p.id} className="lobby-player">
              <span className="color-dot" style={{ backgroundColor: colorMap[p.color] }} />
              <span>{p.name}</span>
              {p.id === lobby.you && <span>(you)</span>}
              {p.id === lobby.host && <span>— host</span>}
            </li>
          ))}
        </ul>
        {lobby.you === lobby.host && lobby.players.length >= 2 && (
          <button onClick={startGame}>Start Game</button>
        )}
        {lobby.you === lobby.host && lobby.players.length < 2 && (
          <p>Waiting for more players…</p>
        )}
      </div>
    );
  }

  if (game) {
    return (
      <GameScreen
        game={game}
        log={log}
        send={sendAction}
        pending={pending}
        setPending={setPending}
        robberTile={robberTile}
        setRobberTile={setRobberTile}
      />
    );
  }

  return (
    <div className="app">
      <h1>Catan</h1>
      {error && (
        <div className="toast">
          {error}
          <button onClick={clearError}>×</button>
        </div>
      )}
      <div className="lobby-form">
        <input placeholder="Your name" value={name} onChange={(e) => setName(e.target.value)} />
        <button onClick={createGame}>Create Game</button>
        <input placeholder="Room code" value={joinCode} onChange={(e) => setJoinCode(e.target.value)} />
        <button onClick={joinGame}>Join Game</button>
      </div>
    </div>
  );
}

export default App;
