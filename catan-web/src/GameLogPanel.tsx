import { useEffect, useMemo, useRef } from 'react';
import type {
  Board,
  GameEvent,
  GameLog,
  PlayerColor,
  PlayerView,
  Resource,
  ResourceDelta,
  Resources,
} from './types';

interface GameLogPanelProps {
  log: GameLog | null;
  players: PlayerView[];
  board: Board;
}

// Text colors per player; 'White' uses a darker shade so it stays readable on
// the light panel background.
const nameColor: Record<PlayerColor, string> = {
  Red: '#d32f2f',
  Blue: '#1976d2',
  White: '#8a8a7a',
  Orange: '#f57c00',
};

const RESOURCE_ICONS: Record<Resource, string> = {
  Brick: '#c1443c',
  Lumber: '#4a8f4a',
  Ore: '#8c8c8c',
  Grain: '#e0b83a',
  Wool: '#7fbf7f',
};

function formatResources(r: Resources): string {
  const entries = (Object.keys(RESOURCE_ICONS) as Resource[])
    .map((res) => ({ res, count: r[res.toLowerCase() as keyof Resources] }))
    .filter((e) => e.count > 0);
  return entries.length > 0
    ? entries.map((e) => `${e.count} ${e.res.toLowerCase()}`).join(', ')
    : 'nothing';
}

function outcomeResourceText(outcome: GameEvent['outcome']): string {
  if (typeof outcome === 'object' && 'ResourceStolen' in outcome) {
    return ` a ${outcome.ResourceStolen.resource.toLowerCase()}`;
  }
  return ' a resource';
}

// Sorted dice numbers of the tiles adjacent to a vertex, e.g. " (on 3, 4, 8)".
function adjacentNumbers(board: Board, vertexId: number): string {
  const vertex = board.vertices[vertexId];
  if (!vertex) return '';
  const numbers = vertex.incident_tiles
    .map((tileId) => board.tiles[tileId]?.number)
    .filter((n): n is number => n !== null && n !== undefined)
    .sort((a, b) => a - b);
  return numbers.length > 0 ? ` (on ${numbers.join(', ')})` : '';
}

function eventResourceChanges(event: GameEvent): ResourceDelta[] | null {
  if (event.resource_changes && event.resource_changes.length > 0) {
    return event.resource_changes;
  }
  if (typeof event.outcome === 'object' && 'DiceRoll' in event.outcome) {
    const { d1, d2, production } = event.outcome.DiceRoll;
    if (d1 + d2 === 7) return null;
    return (production ?? []).map((payout) => ({
      player: payout.player,
      gained: payout.resources,
      lost: emptyResources(),
    }));
  }
  if (typeof event.outcome === 'object' && 'ResourcesGained' in event.outcome) {
    return [
      {
        player: event.player,
        gained: event.outcome.ResourcesGained.resources,
        lost: emptyResources(),
      },
    ];
  }
  if (event.action === 'RollDice') {
    return [];
  }
  return null;
}

function emptyResources(): Resources {
  return { brick: 0, lumber: 0, ore: 0, grain: 0, wool: 0 };
}

function awardText(
  event: GameEvent,
  nameOf: (id: number) => string,
): string[] {
  const lines: string[] = [];
  for (const change of event.award_changes ?? []) {
    const award = change.award === 'LongestRoad' ? 'Longest Road' : 'Largest Army';
    if (change.holder === null) {
      lines.push(`${award} is no longer held`);
    } else {
      lines.push(`${nameOf(change.holder)} took ${award}`);
    }
  }
  if (event.winner != null) {
    lines.push(`${nameOf(event.winner)} won the game`);
  }
  return lines;
}

function formatTimestamp(ms?: number): string | null {
  if (!ms) return null;
  return new Date(ms).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

function formatEvent(event: GameEvent, nameOf: (id: number) => string, board: Board): string {
  const who = nameOf(event.player);
  const a = event.action;

  if (a === 'RollDice') {
    if (typeof event.outcome === 'object' && 'DiceRoll' in event.outcome) {
      const { d1, d2, discards } = event.outcome.DiceRoll;
      const total = d1 + d2;
      if (total === 7 && discards && discards.length > 0) {
        const whoMust = discards
          .map((demand) => `${nameOf(demand.player)} discards ${demand.count}`)
          .join(', ');
        return `${who} rolled ${d1} and ${d2} (7 total); ${whoMust}`;
      }
      return `${who} rolled ${d1} and ${d2} (${total} total)`;
    }
    return `${who} rolled the dice`;
  }
  if (a === 'BuyDevelopmentCard') return `${who} bought a development card`;
  if (a === 'EndTurn') return `${who} ended their turn`;

  if ('PlaceSettlement' in a)
    return `${who} placed their initial settlement${adjacentNumbers(board, a.PlaceSettlement)}`;
  if ('PlaceRoad' in a) return `${who} placed their initial road`;
  if ('BuildRoad' in a) return `${who} built a road`;
  if ('BuildSettlement' in a)
    return `${who} built a settlement${adjacentNumbers(board, a.BuildSettlement)}`;
  if ('BuildCity' in a) return `${who} upgraded a settlement to a city`;
  if ('Discard' in a) return `${who} discarded ${formatResources(a.Discard)}`;
  if ('MoveRobber' in a) {
    const target = a.MoveRobber.target;
    return target !== null
      ? `${who} moved the robber and stole${outcomeResourceText(event.outcome)} from ${nameOf(target)}`
      : `${who} moved the robber`;
  }
  if ('PlayKnight' in a) {
    const target = a.PlayKnight.target;
    return target !== null
      ? `${who} played a Knight, moved the robber, and stole${outcomeResourceText(event.outcome)} from ${nameOf(target)}`
      : `${who} played a Knight and moved the robber`;
  }
  if ('PlayRoadBuilding' in a) return `${who} played Road Building and built two roads`;
  if ('PlayInvention' in a) {
    const [r1, r2] = a.PlayInvention;
    return `${who} played Invention for ${r1.toLowerCase()} and ${r2.toLowerCase()}`;
  }
  if ('PlayMonopoly' in a) {
    if (typeof event.outcome === 'object' && 'Monopoly' in event.outcome) {
      const { resource, amount } = event.outcome.Monopoly;
      return `${who} played Monopoly on ${resource.toLowerCase()} and took ${amount}`;
    }
    return `${who} played Monopoly on ${a.PlayMonopoly.toLowerCase()}`;
  }
  if ('OfferTrade' in a) {
    return `${who} offered ${formatResources(a.OfferTrade.give)} for ${formatResources(a.OfferTrade.receive)}`;
  }
  if ('RespondTrade' in a) return `${who} ${a.RespondTrade.accept ? 'accepted' : 'declined'} the trade`;
  if ('CancelTrade' in a) return `${who} cancelled the trade offer`;
  if ('BankTrade' in a) {
    return `${who} traded ${formatResources(a.BankTrade.give)} for ${formatResources(a.BankTrade.receive)} with the bank`;
  }
  return `${who} acted`;
}

export function GameLogPanel({ log, players, board }: GameLogPanelProps) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const eventCount = log?.events.length ?? 0;

  const playerInfo = useMemo(() => {
    const map = new Map<number, { name: string; color: PlayerColor }>();
    log?.initial_players.forEach((p) => map.set(p.id, { name: p.name, color: p.color }));
    // Prefer live GameView info (authoritative names/colors).
    players.forEach((p) => map.set(p.id, { name: p.name, color: p.color }));
    return map;
  }, [log, players]);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [eventCount]);

  const nameOf = (id: number) => playerInfo.get(id)?.name ?? `Player ${id}`;

  return (
    <div className="card game-log-panel">
      <h4>Game Log</h4>
      <div className="game-log-list" ref={scrollRef}>
        {!log || log.events.length === 0 ? (
          <p className="game-log-empty">No events yet.</p>
        ) : (
          log.events.map((event) => {
            const info = playerInfo.get(event.player);
            const color = info ? nameColor[info.color] : '#555';
            const line = formatEvent(event, nameOf, board);
            const awards = awardText(event, nameOf);
            const changes = eventResourceChanges(event);
            const time = formatTimestamp(event.timestamp_ms);
            return (
              <div className="game-log-event" key={event.sequence}>
                <p className="game-log-entry">
                  {time && <span className="game-log-time">{time}</span>}
                  <span className="game-log-name" style={{ color }}>
                    {info?.name ?? `Player ${event.player}`}
                  </span>
                  {line.slice((info?.name ?? `Player ${event.player}`).length)}
                </p>
                {changes !== null &&
                  (changes.length === 0 ? (
                    <p className="game-log-entry game-log-detail">nobody received resources</p>
                  ) : (
                    changes.flatMap((change) => {
                      const recipient = playerInfo.get(change.player);
                      const recipientColor = recipient ? nameColor[recipient.color] : '#555';
                      const name = recipient?.name ?? `Player ${change.player}`;
                      const lines: { key: string; suffix: string }[] = [];
                      if (formatResources(change.lost) !== 'nothing') {
                        lines.push({
                          key: `${change.player}-lost`,
                          suffix: ` lost ${formatResources(change.lost)}`,
                        });
                      }
                      if (formatResources(change.gained) !== 'nothing') {
                        lines.push({
                          key: `${change.player}-gained`,
                          suffix: ` received ${formatResources(change.gained)}`,
                        });
                      }
                      return lines.map((detail) => (
                        <p className="game-log-entry game-log-detail" key={detail.key}>
                          <span className="game-log-name" style={{ color: recipientColor }}>
                            {name}
                          </span>
                          {detail.suffix}
                        </p>
                      ));
                    })
                  ))}
                {awards.map((detail) => (
                  <p className="game-log-entry game-log-detail" key={detail}>
                    {detail}
                  </p>
                ))}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
