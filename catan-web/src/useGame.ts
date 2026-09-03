import { useCallback, useEffect, useRef, useState } from 'react';
import type { ClientMessage, GameView, LobbyPlayer, ServerMessage } from './types';

const WS_URL =
  typeof window !== 'undefined' && window.location.protocol === 'https:'
    ? `wss://${window.location.host}/ws`
    : `ws://${window.location.host}/ws`;

export interface GameHook {
  connected: boolean;
  error: string | null;
  lobby: { code: string; players: LobbyPlayer[]; you: number; host: number } | null;
  game: GameView | null;
  send: (msg: ClientMessage) => void;
  clearError: () => void;
}

export function useGame(): GameHook {
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lobby, setLobby] = useState<GameHook['lobby']>(null);
  const [game, setGame] = useState<GameView | null>(null);

  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    const ws = new WebSocket(WS_URL);
    wsRef.current = ws;

    ws.onopen = () => setConnected(true);
    ws.onclose = () => {
      setConnected(false);
      setError('Disconnected from server');
    };
    ws.onerror = () => setError('WebSocket error');
    ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data) as ServerMessage;
        if (msg.type === 'Error') {
          setError(msg.message);
        } else if (msg.type === 'Info') {
          setError(null);
        } else if (msg.type === 'LobbyState') {
          setLobby(msg);
          setError(null);
        } else if (msg.type === 'GameState') {
          setGame(msg.state);
          setLobby(null);
          setError(null);
        }
      } catch (e) {
        setError('Failed to parse server message');
      }
    };

    return () => {
      ws.close();
    };
  }, []);

  const send = useCallback((msg: ClientMessage) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(msg));
    } else {
      setError('Not connected to server');
    }
  }, []);

  const clearError = useCallback(() => setError(null), []);

  return { connected, error, lobby, game, send, clearError };
}
