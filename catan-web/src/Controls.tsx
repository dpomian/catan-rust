import { useEffect, useRef, useState } from 'react';
import type { Action, GameView, PlayerView, Resource, Resources, TradeOffer } from './types';
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
  | { kind: 'invention'; r1?: Resource; r2?: Resource | null };

const RESOURCES: Resource[] = ['Brick', 'Lumber', 'Ore', 'Grain', 'Wool'];

const RESOURCE_COLORS: Record<Resource, string> = {
  Brick: '#c25e00',
  Lumber: '#4a8f4a',
  Ore: '#757575',
  Grain: '#f2c96d',
  Wool: '#7fa8d6',
};

export function Controls({ game, me, send, setPending, pending }: ControlsProps) {
  const isMyTurn = game.current_player === me.id && game.winner === null;
  const phase = game.phase;
  const turnPhase = typeof phase === 'object' && 'Turn' in phase ? phase.Turn : null;
  const setupPhase = typeof phase === 'object' && 'Setup' in phase ? phase.Setup : null;

  const canTrade = turnPhase === 'Main' && isMyTurn;
  const canPlayDev = turnPhase === 'Main' && isMyTurn;
  const canBuyDev = turnPhase === 'Main' && isMyTurn && canAfford(me.my_resources, DEV_CARD_COST);

  const [monopolyResource, setMonopolyResource] = useState<Resource>('Brick');
  const [tradeOpen, setTradeOpen] = useState(false);

  return (
    <div className="controls">
      {tradeOpen && canTrade && (
        <div className="modal-overlay" onClick={() => setTradeOpen(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <TradePanel game={game} me={me} send={send} onClose={() => setTradeOpen(false)} />
          </div>
        </div>
      )}

      {game.winner !== null && <div className="winner">Winner: {game.players[game.winner]?.name}!</div>}

      {game.pending_trade && (
        <TradeOfferCard offer={game.pending_trade} players={game.players} me={me} send={send} />
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

      {canTrade && (
        <div className="actions">
          <button onClick={() => setTradeOpen(true)}>Trade</button>
        </div>
      )}

      <BuildCostsCard />

      {turnPhase === 'Main' && isMyTurn && (
        <div className="actions end-turn-row">
          <button onClick={() => send('EndTurn')}>End Turn</button>
        </div>
      )}
    </div>
  );
}

const EMPTY_RESOURCES: Resources = { brick: 0, lumber: 0, ore: 0, grain: 0, wool: 0 };

const RESOURCE_KEYS: Record<Resource, keyof Resources> = {
  Brick: 'brick',
  Lumber: 'lumber',
  Ore: 'ore',
  Grain: 'grain',
  Wool: 'wool',
};

function totalResources(r: Resources): number {
  return r.brick + r.lumber + r.ore + r.grain + r.wool;
}

function resourceEntries(r: Resources): { resource: Resource; count: number }[] {
  return RESOURCES.map((resource) => ({ resource, count: r[RESOURCE_KEYS[resource]] })).filter(
    (e) => e.count > 0
  );
}

function formatResources(r: Resources): string {
  const entries = resourceEntries(r);
  return entries.length > 0
    ? entries.map((e) => `${e.count} ${e.resource}`).join(', ')
    : 'nothing';
}

function TradePanel({
  game,
  me,
  send,
  onClose,
}: {
  game: GameView;
  me: PlayerView;
  send: (action: Action) => void;
  onClose: () => void;
}) {
  const [mode, setMode] = useState<'bank' | 'player'>('bank');
  const [give, setGive] = useState<Resources>({ ...EMPTY_RESOURCES });
  const [receive, setReceive] = useState<Resources>({ ...EMPTY_RESOURCES });
  const [offerSubmitted, setOfferSubmitted] = useState(false);

  const pendingOffer =
    game.pending_trade && game.pending_trade.from === me.id ? game.pending_trade : null;
  const offerConfirmedRef = useRef(false);

  useEffect(() => {
    if (pendingOffer) {
      offerConfirmedRef.current = true;
    } else if (offerSubmitted && offerConfirmedRef.current) {
      onClose();
    }
  }, [pendingOffer, offerSubmitted, onClose]);

  const giveEntries = resourceEntries(give);
  const receiveEntries = resourceEntries(receive);
  const isValidBankTrade =
    giveEntries.length === 1 &&
    giveEntries[0].count === 4 &&
    receiveEntries.length === 1 &&
    receiveEntries[0].count === 1 &&
    giveEntries[0].resource !== receiveEntries[0].resource;

  const canSubmit =
    totalResources(give) > 0 &&
    totalResources(receive) > 0 &&
    canAfford(me.my_resources, give) &&
    (mode !== 'bank' || isValidBankTrade);

  const submit = () => {
    if (!canSubmit) return;
    if (mode === 'bank') {
      send({ BankTrade: { give, receive } });
      setGive({ ...EMPTY_RESOURCES });
      setReceive({ ...EMPTY_RESOURCES });
      onClose();
    } else {
      send({ OfferTrade: { give, receive } });
      setOfferSubmitted(true);
    }
  };

  const cancel = () => {
    if (pendingOffer) {
      send({ CancelTrade: { trade_id: pendingOffer.trade_id } });
    }
    onClose();
  };

  if (offerSubmitted && pendingOffer) {
    return (
      <div className="card trade-panel">
        <div className="trade-panel-header">
          <h4>Trade Offer Pending</h4>
          <button className="trade-close" onClick={cancel}>
            ×
          </button>
        </div>
        <p className="trade-offer-text">
          You offer {formatResources(pendingOffer.give)} in exchange for{' '}
          {formatResources(pendingOffer.receive)}.
        </p>
        <p className="trade-hint">This offer will expire in 15 seconds unless someone accepts it.</p>
        <div className="actions">
          <button onClick={cancel}>Cancel Offer</button>
        </div>
      </div>
    );
  }

  if (offerSubmitted && !pendingOffer) {
    return (
      <div className="card trade-panel">
        <div className="trade-panel-header">
          <h4>Trade</h4>
          <button className="trade-close" onClick={onClose}>
            ×
          </button>
        </div>
        <p className="trade-hint">Sending offer...</p>
      </div>
    );
  }

  return (
    <div className="card trade-panel">
      <div className="trade-panel-header">
        <h4>Trade</h4>
        <button className="trade-close" onClick={onClose}>×</button>
      </div>
      <div className="trade-tabs">
        <button className={mode === 'bank' ? 'active' : ''} onClick={() => setMode('bank')}>
          Bank
        </button>
        <button className={mode === 'player' ? 'active' : ''} onClick={() => setMode('player')}>
          Player
        </button>
      </div>
      <div className="trade-bundles">
        <ResourceBundleEditor label="You give" resources={give} maxResources={me.my_resources} onChange={setGive} />
        <div className="trade-exchange-arrow">&darr; in exchange for &darr;</div>
        <ResourceBundleEditor label="You receive" resources={receive} onChange={setReceive} />
        <button className="trade-submit" onClick={submit} disabled={!canSubmit}>
          {mode === 'bank' ? 'Trade with Bank' : 'Offer to Players'}
        </button>
        {mode === 'bank' && (
          <p className="trade-hint">Bank rate is always 4:1 — give 4 of one resource for 1 of another.</p>
        )}
      </div>
    </div>
  );
}

function ResourceBundleEditor({
  label,
  resources,
  maxResources,
  onChange,
}: {
  label: string;
  resources: Resources;
  maxResources?: Resources | null;
  onChange: (resources: Resources) => void;
}) {
  return (
    <div className="trade-bundle-editor">
      <div className="trade-bundle-label">{label}</div>
      <div className="trade-resource-rows">
        {RESOURCES.map((resource) => {
          const key = RESOURCE_KEYS[resource];
          const count = resources[key];
          const max = maxResources ? maxResources[key] : undefined;
          return (
            <label key={resource} className="trade-resource-row">
              <span className="trade-resource-name">{resource}</span>
              <input
                type="number"
                min={0}
                max={max ?? 99}
                value={count}
                onChange={(e) => {
                  let value = parseInt(e.target.value, 10);
                  if (Number.isNaN(value)) value = 0;
                  value = Math.max(0, value);
                  if (max !== undefined) value = Math.min(value, max);
                  onChange({ ...resources, [key]: value });
                }}
              />
            </label>
          );
        })}
      </div>
    </div>
  );
}

function TradeOfferCard({
  offer,
  players,
  me,
  send,
}: {
  offer: TradeOffer;
  players: PlayerView[];
  me: PlayerView;
  send: (action: Action) => void;
}) {
  const from = players.find((p) => p.id === offer.from);
  const isInitiator = me.id === offer.from;

  if (!isInitiator && offer.rejected_by.includes(me.id)) {
    return null;
  }

  const offerText = `${isInitiator ? 'You' : from?.name ?? 'Unknown'} offers ${formatResources(offer.give)} in exchange for ${formatResources(offer.receive)}`;

  return (
    <div className="card trade-offer-card">
      <h4>Trade Offer</h4>
      <p className="trade-offer-text">{offerText}</p>
      {!isInitiator && (
        <div className="actions">
          <button onClick={() => send({ RespondTrade: { accept: true } })}>Accept</button>
          <button onClick={() => send({ RespondTrade: { accept: false } })}>Reject</button>
        </div>
      )}
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


