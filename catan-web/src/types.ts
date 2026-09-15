export type PlayerColor = 'Red' | 'Blue' | 'White' | 'Orange';
export type Terrain = 'Hills' | 'Forest' | 'Mountains' | 'Fields' | 'Pasture' | 'Desert';
export type Resource = 'Brick' | 'Lumber' | 'Ore' | 'Grain' | 'Wool';
export type DevelopmentCard = 'Knight' | 'VictoryPoint' | 'RoadBuilding' | 'Invention' | 'Monopoly';
export type TurnPhase = 'Roll' | 'ResolveSeven' | 'Main';

export type GamePhase =
  | { Setup: { round: number; reverse: boolean; awaiting_road_for: number | null } }
  | { Turn: TurnPhase }
  | 'Finished';

export type Port = 'Generic3To1' | { Specific2To1: Resource };
export type Building = { Settlement: number } | { City: number };

export interface Resources {
  brick: number;
  lumber: number;
  ore: number;
  grain: number;
  wool: number;
}

export interface Tile {
  id: number;
  coord: { x: number; y: number; z: number };
  terrain: Terrain;
  number: number | null;
  has_robber: boolean;
  vertices: number[];
  edges: number[];
}

export interface Vertex {
  id: number;
  position: { x: number; y: number };
  incident_tiles: number[];
  incident_edges: number[];
  port: Port | null;
  building: Building | null;
}

export interface Edge {
  id: number;
  endpoints: [number, number];
  incident_tiles: number[];
  road: number | null;
}

export interface Board {
  tiles: Tile[];
  vertices: Vertex[];
  edges: Edge[];
}

export interface PlayerView {
  id: number;
  name: string;
  color: PlayerColor;
  resource_count: number;
  dev_card_count: number;
  victory_points: number;
  played_knights: number;
  remaining_roads: number;
  remaining_settlements: number;
  remaining_cities: number;
  is_me: boolean;
  my_resources?: Resources;
  my_hand?: DevelopmentCard[];
  my_new_cards?: DevelopmentCard[];
  my_victory_point_cards?: number;
}

export interface TradeOffer {
  trade_id: number;
  from: number;
  give: Resources;
  receive: Resources;
  rejected_by: number[];
}

export interface GameView {
  board: Board;
  players: PlayerView[];
  my_id: number;
  current_player: number;
  phase: GamePhase;
  dice: [number, number] | null;
  longest_road: number | null;
  largest_army: number | null;
  pending_trade: TradeOffer | null;
  pending_discards: Record<number, number>;
  winner: number | null;
}

export interface LogPlayer {
  id: number;
  name: string;
  color: PlayerColor;
  resources: Resources;
  hand: DevelopmentCard[];
  new_cards: DevelopmentCard[];
  played_knights: number;
  victory_point_cards: number;
  remaining_roads: number;
  remaining_settlements: number;
  remaining_cities: number;
}

export interface ResourcePayout {
  player: number;
  resources: Resources;
}

export interface ResourceDelta {
  player: number;
  gained: Resources;
  lost: Resources;
}

export type EventOutcome =
  | 'None'
  | { DiceRoll: { d1: number; d2: number; production?: ResourcePayout[] } }
  | { ResourceStolen: { resource: Resource } }
  | { ResourcesGained: { resources: Resources } };

export interface GameEvent {
  sequence: number;
  player: number;
  action: Action;
  outcome: EventOutcome;
  resource_changes?: ResourceDelta[];
  timestamp_ms?: number;
}

export interface GameLog {
  seed: number;
  initial_players: LogPlayer[];
  initial_player_order: number[];
  events: GameEvent[];
}

export type ServerMessage =
  | { type: 'Error'; message: string }
  | { type: 'LobbyState'; code: string; players: LobbyPlayer[]; you: number; host: number }
  | { type: 'GameState'; state: GameView }
  | { type: 'GameLog'; log: GameLog }
  | { type: 'Info'; message: string };

export interface LobbyPlayer {
  id: number;
  name: string;
  color: PlayerColor;
}

export type ClientMessage =
  | { type: 'CreateGame'; name: string }
  | { type: 'JoinGame'; code: string; name: string }
  | { type: 'StartGame' }
  | { type: 'Action'; action: Action }
  | { type: 'RequestLog' };

export type Action =
  | { PlaceSettlement: number }
  | { PlaceRoad: number }
  | 'RollDice'
  | { MoveRobber: { tile: number; target: number | null } }
  | { Discard: Resources }
  | { BuildRoad: number }
  | { BuildSettlement: number }
  | { BuildCity: number }
  | 'BuyDevelopmentCard'
  | { PlayKnight: { tile: number; target: number | null } }
  | { PlayRoadBuilding: [number, number] }
  | { PlayInvention: [Resource, Resource] }
  | { PlayMonopoly: Resource }
  | { OfferTrade: { give: Resources; receive: Resources } }
  | { RespondTrade: { accept: boolean } }
  | { CancelTrade: { trade_id: number } }
  | { BankTrade: { give: Resources; receive: Resources } }
  | 'EndTurn';
