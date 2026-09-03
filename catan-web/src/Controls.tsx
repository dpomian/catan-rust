import { useState } from 'react';
import type { Action, GameView, PlayerView, Resource, Resources } from './types';
import {
  canAfford,
  CITY_COST,
  DEV_CARD_COST,
  ROAD_COST,
  SETTLEMENT_COST,
} from './build-helpers';

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

const RESOURCE_COLORS: Record<Resource, string> = {
  Brick: '#c25e00',
  Lumber: '#4a8f4a',
  Ore: '#757575',
  Grain: '#f2c96d',
  Wool: '#7fa8d6',
};

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
  const isMyTurn = game.current_player === me.id && game.winner === null;
  const phase = game.phase;
  const turnPhase = typeof phase === 'object' && 'Turn' in phase ? phase.Turn : null;
  const setupPhase = typeof phase === 'object' && 'Setup' in phase ? phase.Setup : null;

  const canTrade = turnPhase === 'Main' && isMyTurn;
  const canPlayDev = turnPhase === 'Main' && isMyTurn;
  const canBuyDev = turnPhase === 'Main' && isMyTurn && canAfford(me.my_resources, DEV_CARD_COST);

  const [monopolyResource, setMonopolyResource] = useState<Resource>('Brick');

  return (
    <div className="controls">
      {game.winner !== null && <div className="winner">Winner: {game.players[game.winner]?.name}!</div>}

      {game.pending_trade && game.pending_trade.to === me.id && (
        <div className="card">
          <h4>Trade offer from {game.players[game.pending_trade.from]?.name}</h4>
          <div>
            Give: {fmtResources(game.pending_trade.give)} &rarr; Receive: {fmtResources(game.pending_trade.receive)}
          </div>
          <div className="actions">
            <button onClick={() => send({ RespondTrade: { accept: true } })}>Accept</button>
            <button onClick={() => send({ RespondTrade: { accept: false } })}>Reject</button>
          </div>
        </div>
      )}

      {setupPhase && isMyTurn && setupPhase.awaiting_road_for === null && (
        <div className="pending">Click a highlighted vertex to place your settlement.</div>
      )}
      {setupPhase && isMyTurn && setupPhase.awaiting_road_for !== null && (
        <div className="pending">Click a highlighted edge to place your road.</div>
      )}

      {turnPhase === 'Roll' && isMyTurn && (
        <div className="actions">
          <button onClick={() => send('RollDice')}>Roll Dice</button>
        </div>
      )}

      {turnPhase === 'ResolveSeven' && game.pending_discards[me.id] !== undefined && (
        <DiscardPicker me={me} required={game.pending_discards[me.id]} send={send} />
      )}
      {turnPhase === 'ResolveSeven' && isMyTurn && Object.keys(game.pending_discards).length > 0 && (
        <div className="pending">Waiting for required discards.</div>
      )}
      {turnPhase === 'ResolveSeven' && isMyTurn && Object.keys(game.pending_discards).length === 0 && (
        <div className="pending">Click a tile to move the robber, then choose a victim.</div>
      )}

      {pending?.kind === 'robber' && <div className="pending">Click a tile to place the robber.</div>}
      {pending?.kind === 'knight' && <div className="pending">Click a tile to move the robber (Knight).</div>}
      {pending?.kind === 'roadBuilding' && (
        <div className="pending">Click {2 - pending.edges.length} highlighted edge(s) to build roads.</div>
      )}
      {pending?.kind === 'invention' && <InventionPicker send={send} setPending={setPending} />}

      {canPlayDev && (
        <div className="card dev-card-panel">
          <h4>Development Cards</h4>
          <div className="actions">
            <button onClick={() => send('BuyDevelopmentCard')} disabled={!canBuyDev}>
              Buy Dev Card
            </button>
            <button onClick={() => setPending({ kind: 'knight' })}>Play Knight</button>
            <button onClick={() => setPending({ kind: 'roadBuilding', edges: [] })}>
              Road Building
            </button>
            <button onClick={() => setPending({ kind: 'invention' })}>Invention</button>
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
        </div>
      )}

      {canTrade && <TradePanel game={game} me={me} send={send} />}

      <BuildCostsCard />

      <div className="card resource-hand-card">
        <h4>Resources</h4>
        <ResourceCards resources={me.my_resources} />
      </div>

      <div className="card dev-hand-card">
        <h4>Development Cards</h4>
        {me.my_hand && me.my_hand.length > 0 ? me.my_hand.join(', ') : 'none'}
        {me.my_new_cards && me.my_new_cards.length > 0 && (
          <span> (new: {me.my_new_cards.join(', ')})</span>
        )}
      </div>

      {turnPhase === 'Main' && isMyTurn && (
        <div className="actions end-turn-row">
          <button onClick={() => send('EndTurn')}>End Turn</button>
        </div>
      )}
    </div>
  );
}

function TradePanel({ game, me, send }: { game: GameView; me: PlayerView; send: (action: Action) => void }) {
  const [mode, setMode] = useState<'bank' | 'player'>('bank');
  const [give, setGive] = useState<Resource>('Brick');
  const [receive, setReceive] = useState<Resource>('Lumber');
  const [to, setTo] = useState<number>(game.players.find((p) => p.id !== me.id)?.id ?? 0);

  const others = game.players.filter((p) => p.id !== me.id);

  return (
    <div className="card trade-panel">
      <h4>Trade</h4>
      <div className="trade-tabs">
        <button className={mode === 'bank' ? 'active' : ''} onClick={() => setMode('bank')}>
          Bank
        </button>
        <button className={mode === 'player' ? 'active' : ''} onClick={() => setMode('player')}>
          Player
        </button>
      </div>
      <div className="trade-fields">
        {mode === 'player' && (
          <select value={to} onChange={(e) => setTo(parseInt(e.target.value))}>
            {others.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        )}
        <span>give</span>
        <select value={give} onChange={(e) => setGive(e.target.value as Resource)}>
          {RESOURCES.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </select>
        <span>for</span>
        <select value={receive} onChange={(e) => setReceive(e.target.value as Resource)}>
          {RESOURCES.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </select>
        <button
          onClick={() =>
            mode === 'bank'
              ? send({ BankTrade: { give, receive } })
              : send({
                  OfferTrade: {
                    to,
                    give: amountOf(give, 1),
                    receive: amountOf(receive, 1),
                  },
                })
          }
        >
          {mode === 'bank' ? 'Trade' : 'Offer'}
        </button>
      </div>
    </div>
  );
}

function BuildCostsCard() {
  return (
    <div className="card build-costs-card">
      <h4>Build Costs</h4>
      <div className="build-cost-groups">
        <CostGroup name="Road" cost={ROAD_COST} />
        <CostGroup name="Settlement" cost={SETTLEMENT_COST} />
        <CostGroup name="City" cost={CITY_COST} />
        <CostGroup name="Dev Card" cost={DEV_CARD_COST} />
      </div>
    </div>
  );
}

function CostGroup({ name, cost }: { name: string; cost: Resources }) {
  const entries = RESOURCES.map((res) => {
    const key: keyof Resources =
      res === 'Brick' ? 'brick' : res === 'Lumber' ? 'lumber' : res === 'Ore' ? 'ore' : res === 'Grain' ? 'grain' : 'wool';
    return { res, count: cost[key] };
  }).filter((e) => e.count > 0);

  return (
    <div className="build-cost-group" data-building={name.toLowerCase().replace(' ', '-')}>
      <div className="build-cost-label">{name}</div>
      <div className="build-cost-resources">
        {entries.map(({ res, count }) => (
          <ResourceCard key={res} resource={res} count={count} mini />
        ))}
      </div>
    </div>
  );
}

function ResourceCards({ resources }: { resources?: Resources | null }) {
  if (!resources) {
    return <span className="muted">Hidden</span>;
  }
  return (
    <div className="resource-cards">
      {RESOURCES.map((res) => {
        const key: keyof Resources =
          res === 'Brick' ? 'brick' : res === 'Lumber' ? 'lumber' : res === 'Ore' ? 'ore' : res === 'Grain' ? 'grain' : 'wool';
        return <ResourceCard key={res} resource={res} count={resources[key]} />;
      })}
    </div>
  );
}

function ResourceCard({ resource, count, mini }: { resource: Resource; count: number; mini?: boolean }) {
  return (
    <div
      className={`resource-card resource-card-${resource.toLowerCase()} ${count === 0 ? 'zero' : ''} ${mini ? 'resource-card-mini' : ''}`}
      data-resource={resource}
    >
      <span className="resource-card-icon" style={{ backgroundColor: RESOURCE_COLORS[resource] }} />
      <span className="resource-card-name">{resource}</span>
      <span className="resource-card-count">{count}</span>
    </div>
  );
}

function DiscardPicker({ me, required, send }: { me: PlayerView; required: number; send: (a: Action) => void }) {
  const [resources, setResources] = useState<Resources>({ brick: 0, lumber: 0, ore: 0, grain: 0, wool: 0 });
  const keys: Record<Resource, keyof Resources> = {
    Brick: 'brick',
    Lumber: 'lumber',
    Ore: 'ore',
    Grain: 'grain',
    Wool: 'wool',
  };
  const total = Object.values(resources).reduce((sum, amount) => sum + amount, 0);
  return (
    <div className="pending">
      <div>Discard exactly {required} resources.</div>
      {RESOURCES.map((resource) => {
        const key = keys[resource];
        return (
          <label key={resource}>
            {resource}
            <input
              type="number"
              min={0}
              max={me.my_resources?.[key] ?? 0}
              value={resources[key]}
              onChange={(event) => setResources({ ...resources, [key]: Number(event.target.value) })}
            />
          </label>
        );
      })}
      <button disabled={total !== required} onClick={() => send({ Discard: resources })}>Discard</button>
    </div>
  );
}

function InventionPicker({ send, setPending }: { send: (a: Action) => void; setPending: (p: null) => void }) {
  const [r1, setR1] = useState<Resource>('Brick');
  const [r2, setR2] = useState<Resource>('Brick');
  return (
    <div className="pending">
      <select value={r1} onChange={(e) => setR1(e.target.value as Resource)}>
        {RESOURCES.map((r) => (
          <option key={r} value={r}>
            {r}
          </option>
        ))}
      </select>
      <select value={r2} onChange={(e) => setR2(e.target.value as Resource)}>
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
