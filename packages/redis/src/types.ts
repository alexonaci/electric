/**
 * @electric-sql/redis - Type definitions
 *
 * This module defines interfaces that are compatible with multiple Redis client libraries:
 * - node-redis (redis package)
 * - ioredis
 * - iovalkey
 *
 * Users can pass any Redis client that satisfies these interfaces.
 */

import type { ShapeStreamOptions } from '@electric-sql/client'

/**
 * Electric options that can be passed through to ShapeStream.
 * Excludes 'params' and 'signal' which are managed by the library.
 */
export type ElectricOptions = Omit<ShapeStreamOptions, `params` | `signal`>

/**
 * Shape parameters passed to Electric's ShapeStream.
 * Extracted from ShapeStreamOptions['params'].
 */
export type ShapeParams = ShapeStreamOptions[`params`]

// ============================================================================
// Redis Client Interface Abstraction
// ============================================================================

/**
 * Sorted set member with score for zAdd operations
 */
export interface RedisSortedSetMember {
  score: number
  value: string
}

// ============================================================================
// Redis Pipeline Interface (Simplified with Lua Scripts)
// ============================================================================

/**
 * Minimal pipeline interface for Lua script execution.
 * Using Lua scripts means we only need evalsha/evalSha - no other methods!
 * All Redis operations are performed via EVALSHA for client-agnostic behavior.
 * @internal
 */
export interface RedisMultiLike {
  // node-redis style
  evalSha?(sha1: string, options: { keys: string[]; arguments: string[] }): this
  // ioredis/iovalkey style
  evalsha?(sha1: string, numKeys: number, ...args: string[]): this

  // Execute the pipeline
  exec(): Promise<unknown[]>
}

/**
 * Internal interface used by the library for Redis operations.
 * This is what the library expects after accepting any Redis client.
 * Only includes write operations - users read directly from their client.
 * @internal
 */
export interface RedisClientLike {
  connect(): Promise<unknown>
  del(...args: any[]): Promise<number>
  SCRIPT_LOAD?(...args: any[]): Promise<string>
  scriptLoad?(...args: any[]): Promise<string>
  script?(...args: any[]): Promise<unknown>
  multi(...args: any[]): RedisMultiLike
}

/**
 * Minimal Redis client requirements for ElectricRedis.
 *
 * This interface is intentionally loose to accept any Redis client library:
 * - node-redis (redis package)
 * - ioredis
 * - iovalkey
 *
 * The library performs runtime duck-typing to call the appropriate methods.
 *
 * @example
 * ```typescript
 * import { createClient } from 'redis'
 * const redis = createClient()
 * const sync = new ElectricRedis({ redis: client, electric: { url: '...' } })
 *
 * import Redis from 'ioredis'
 * const redis = new Redis()
 * const sync = new ElectricRedis({ redis, electric: { url: '...' } })
 *
 * import Valkey from 'iovalkey'
 * const redis = new Valkey()
 * const sync = new ElectricRedis({ redis, electric: { url: '...' } })
 * ```
 */
export interface RedisClient {
  /** Connect to Redis (node-redis requires this, ioredis auto-connects) */
  connect(): Promise<unknown>
  /** Delete a key */
  del(key: string): Promise<number>
  /** Create a pipeline/transaction */
  multi(...args: unknown[]): unknown
}

// ============================================================================
// Configuration Types
// ============================================================================

/**
 * Redis data structures supported for syncing
 *
 * - `hash`: Key-value store for full record data (default, most common)
 * - `set`: Membership only - stores just the record keys
 * - `sorted_set`: Ordered by score - great for leaderboards, time series
 * - `stream`: Append-only log of all changes (event sourcing)
 */
export type RedisStructure = `hash` | `set` | `sorted_set` | `stream`

// ============================================================================
// Shape Configuration - Type-safe per structure
// ============================================================================

/**
 * Base shape configuration shared by all structures
 */
interface BaseShapeConfig {
  /** Electric/Postgres shape parameters - passed directly to ShapeStream */
  shape: ShapeParams
  /** Primary key field name in the data (default: 'id') */
  key?: string
}

/**
 * Hash structure config - stores records as Redis hash fields
 */
export interface HashShapeConfig extends BaseShapeConfig {
  structure?: `hash`
}

/**
 * Set structure config - stores only keys (membership)
 */
export interface SetShapeConfig extends BaseShapeConfig {
  structure: `set`
}

/**
 * Sorted set structure config - stores records ordered by score
 * Score field is REQUIRED - no fallback to Date.now()
 */
export interface SortedSetShapeConfig extends BaseShapeConfig {
  structure: `sorted_set`
  /**
   * Field to use as score (REQUIRED for sorted_set)
   * The field value must be numeric. Throws if missing or non-numeric.
   */
  score: string
}

/**
 * Stream structure config - append-only log of changes
 */
export interface StreamShapeConfig extends BaseShapeConfig {
  structure: `stream`
}

/**
 * Union type for all shape configurations
 * Type-safe: each structure has its own required/optional fields
 */
export type ShapeConfig =
  | HashShapeConfig
  | SetShapeConfig
  | SortedSetShapeConfig
  | StreamShapeConfig

/**
 * Internal shape config used after normalization
 * @internal
 */
export interface InternalShapeConfig {
  shape: ShapeParams
  structure: RedisStructure
  key: string
  score?: string
}

/**
 * Configuration for ElectricRedis
 */
export interface ElectricRedisConfig {
  /**
   * Electric SQL configuration - URL and optional ShapeStream options
   */
  electric: {
    /** Electric SQL endpoint URL */
    url: string
  } & ElectricOptions

  /**
   * Redis client instance - compatible with node-redis, ioredis, or iovalkey.
   * The library will automatically detect and use the appropriate methods.
   */
  redis: RedisClient

  /**
   * Batch size for Redis operations (default: 1000)
   */
  batchSize?: number
}

// ============================================================================
// Shape Handle Types (Simplified - single type for all structures)
// ============================================================================

/**
 * Unsubscribe function returned by event handlers
 */
export type Unsubscribe = () => void
