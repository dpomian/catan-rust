import { useState } from 'react';
import type { Action, GameView, PlayerView, Resource, Resources } from './types';

interface ControlsProps {
  game: GameView;
  me: PlayerView;
  send: (action: Action) => void;
  setPending: (pending: Pending | null) => void;
  pending: Pending | null;
}

export type Pending =
  | { kind: 'robber'; tile?: number; target?: number | null }
  | { kind: 'knight'; tile?: number; target?: number | null }
  | { kind: 'roadBuilding'; edges: number[] }
  | { kind: 'invention'; r1?: Resource; r2?: Resource | null }
  | { kind: 'tradePlayer'; to?: number; give: Resource; receive: Resource };

const RESOURCES: Resource[] = ['Brick', 'Lumber', 'Ore', 'Grain', 'Wool'];

function amountOf(resource: Resource, n: number): Resources {
  return {
    brick: resource === 'Brick' ? n : 0,
    lumber: resource === 'Lumber' ? n : 0,
    ore: resource === 'Ore' ? n : 0,
    grain: resource === 'Grain' ? n : 0,
    wool: resource === 'Wool' ? n : 0,
  };
}

export function Controls({ game, me, send, setPending, pending }: ControlsProps) {
  const isMyTurn = game.current_player === me.id && !game.winner;
  const phase = game.phase;
  const turnPhase = typeof phase === 'object' && 'Turn' in phase ? phase.Turn : null;
  const setupPhase = typeof phase === 'object' && 'Setup' in phase ? phase.Setup : null;

  const [bankGive, setBankGive] = useState<Resource>('Brick');
  const [bankReceive, setBankReceive] = useState<Resource>('Lumber');
  const [tradePlayer, setTradePlayer] = useState<number>(0);
  const [tradeGive, setTradeGive] = useState<Resource>('Brick');
  const [tradeReceive, setTradeReceive] = useState<Resource>('Lumber');
  const [monopolyResource, setMonopolyResource] = useState<Resource>('Brick');

  const canAct = isMyTurn;

  const onRoll = () => send('RollDice');
  const onEnd = () => send('EndTurn');
  const onBuyDev = () => send('BuyDevelopmentCard');
  const onBankTrade = () => {
    send({ BankTrade: { give: bankGive, receive: bankReceive } });
  };

  return (
    <div className="controls">
      {game.winner !== null && (
        <div className="winner">
          Winner: {game.players[game.winner!]?.name}!
        </div>
      )}

      {game.pending_trade && game.pending_trade.to === me.id && (
        <div className="trade-offer">
          <strong>Trade offer from {game.players[game.pending_trade.from]?.name}</strong>
          <div>
            Give: {fmtResources(game.pending_trade.give)} &rarr; Receive: {fmtResources(game.pending_trade.receive)}
          </div>
          <button onClick={() => send({ RespondTrade: { accept: true } })}>Accept</button>
          <button onClick={() => send({ RespondTrade: { accept: false } })}>Reject</button>
        </div>
      )}

      {setupPhase && canAct && setupPhase.awaiting_road_for === null && (
        <p>Click an empty vertex to place your settlement.</p>
      )}
      {setupPhase && canAct && setupPhase.awaiting_road_for !== null && (
        <p>Click an adjacent edge to place your road.</p>
      )}

      {turnPhase === 'Roll' && canAct && (
        <div className="actions">
          <button onClick={onRoll}>Roll Dice</button>
        </div>
      )}

      {turnPhase === 'ResolveSeven' && canAct && (
        <p>Click a tile to move the robber. Then choose a victim.</p>
      )}

      {turnPhase === 'Main' && canAct && (
        <div className="actions">
          <button onClick={onEnd}>End Turn</button>
          <button onClick={onBuyDev}>Buy Development Card</button>
          <button onClick={() => setPending({ kind: 'knight' })}>Play Knight</button>
          <button onClick={() => setPending({ kind: 'roadBuilding', edges: [] })}>Play Road Building</button>
          <button onClick={() => setPending({ kind: 'invention' })}>Play Invention</button>
          <span>
            <select value={monopolyResource} onChange={(e) => setMonopolyResource(e.target.value as Resource)}>
              {RESOURCES.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </select>
            <button onClick={() => send({ PlayMonopoly: monopolyResource })}>Monopoly</button>
          </span>
        </div>
      )}

      {turnPhase === 'Main' && canAct && (
        <div className="trade">
          <h4>Bank Trade</h4>
          <select value={bankGive} onChange={(e) => setBankGive(e.target.value as Resource)}>
            {RESOURCES.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
          <span> for </span>
          <select value={bankReceive} onChange={(e) => setBankReceive(e.target.value as Resource)}>
            {RESOURCES.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
          <button onClick={onBankTrade}>Trade</button>
        </div>
      )}

      {turnPhase === 'Main' && canAct && (
        <div className="trade">
          <h4>Trade with Player</h4>
          <select value={tradePlayer} onChange={(e) => setTradePlayer(parseInt(e.target.value))}>
            {game.players
              .filter((p) => p.id !== me.id)
              .map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
          </select>
          <span> give </span>
          <select value={tradeGive} onChange={(e) => setTradeGive(e.target.value as Resource)}>
            {RESOURCES.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
          <span> for </span>
          <select value={tradeReceive} onChange={(e) => setTradeReceive(e.target.value as Resource)}>
            {RESOURCES.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
          <button onClick={() => send({ OfferTrade: { to: tradePlayer, give: amountOf(tradeGive, 1), receive: amountOf(tradeReceive, 1) } })}>Offer</button>
        </div>
      )}

      {pending?.kind === 'robber' && (
        <div className="pending">Click a tile to place the robber.</div>
      )}
      {pending?.kind === 'knight' && (
        <div className="pending">Click a tile to move the robber (Knight).</div>
      )}
      {pending?.kind === 'roadBuilding' && (
        <div className="pending">
          Click {2 - pending.edges.length} edge(s) to build roads.
        </div>
      )}
      {pending?.kind === 'invention' && <InventionPicker send={send} setPending={setPending} />}

      <div className="hand">
        <strong>Resources:</strong> {me.my_resources ? fmtResources(me.my_resources) : ''}
      </div>
      <div className="hand">
        <strong>Dev Cards:</strong>{' '}
        {me.my_hand?.join(', ') || 'none'}
        {me.my_new_cards && me.my_new_cards.length > 0 && (
          <span> (new: {me.my_new_cards.join(', ')})</span>
        )}
      </div>
    </div>
  );
}

function InventionPicker({ send, setPending }: { send: (a: Action) => void; setPending: (p: null) => void }) {
  const [r1, setR1] = useState<Resource>('Brick');
  const [r2, setR2] = useState<Resource | null>(null);
  return (
    <div className="pending">
      <select value={r1} onChange={(e) => setR1(e.target.value as Resource)}>
        {RESOURCES.map((r) => (
          <option key={r} value={r}>
            {r}
          </option>
        ))}
      </select>
      <select value={r2 ?? ''} onChange={(e) => setR2(e.target.value ? (e.target.value as Resource) : null)}>
        <option value="">None</option>
        {RESOURCES.map((r) => (
          <option key={r} value={r}>
            {r}
          </option>
        ))}
      </select>
      <button
        onClick={() => {
          send({ PlayInvention: [r1, r2] });
          setPending(null);
        }}
      >
        Take
      </button>
    </div>
  );
}

function fmtResources(r: Resources): string {
  const map: Record<Resource, keyof Resources> = {
    Brick: 'brick',
    Lumber: 'lumber',
    Ore: 'ore',
    Grain: 'grain',
    Wool: 'wool',
  };
  return RESOURCES.map((res) => `${res}: ${r[map[res]]}`).join(', ');
}
