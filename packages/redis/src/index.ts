/**
 * @electric-sql/redis - Redis integration for Electric SQL
 *
 * Sync Electric SQL Shapes to Redis data structures in real-time.
 *
 * Supported structures: Hash, Set, Sorted Set, Stream
 * Compatible clients: node-redis, ioredis, iovalkey
 *
 * @example
 * ```typescript
 * import { createClient } from 'redis'
 * import { ElectricRedis } from '@electric-sql/redis'
 *
 * const redis = createClient({ url: 'redis://localhost:6379' })
 * const sync = new ElectricRedis({
 *   electric: { url: 'http://localhost:3000/v1/shape' },
 *   redis,
 * })
 *
 * const users = sync.syncShape<User>('users', {
 *   shape: { table: 'users' },
 *   structure: 'hash'
 * })
 *
 * users.onInsert((key, user) => console.log('New user:', user.name))
 * await sync.start()
 *
 * // Read with your Redis client:
 * const allUsers = await redis.hGetAll('users')
 * ```
 */

// Main class
export { ElectricRedis } from './electric-redis-sync'

// Shape handle
export { ShapeHandle } from './shape-handles'

// Configuration types
export type {
  ElectricRedisConfig,
  ShapeParams,
  HashShapeConfig,
  SetShapeConfig,
  SortedSetShapeConfig,
  StreamShapeConfig,
  ShapeConfig,
  RedisStructure,
  RedisClientLike,
  RedisMultiLike,
  RedisSortedSetMember,
  Unsubscribe,
} from './types'

// Lua scripts (for advanced usage)
export {
  LUA_SCRIPTS,
  loadAllScripts,
  type ScriptName,
  type ScriptShaCache,
} from './lua-scripts'

// Utilities
export { chunk, loadScript, pipelineEvalSha } from './utils'

// Handlers (for advanced usage)
export {
  handleOperation,
  handleHash,
  handleStream,
  handleSet,
  handleSortedSet,
  type Operation,
} from './handlers'

// Re-export Electric types
export type { ShapeStreamOptions, Message } from '@electric-sql/client'
