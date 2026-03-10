/**
 * Lua scripts for Redis operations
 *
 * Using Lua scripts provides a client-agnostic interface - all Redis clients
 * (node-redis, ioredis, iovalkey) support EVALSHA with the same semantics.
 *
 * This eliminates the need for duck-typing method names (hSet vs hset, etc.)
 */

import type { RedisClientLike } from './types'
import { loadScript } from './utils'

/**
 * All Lua scripts used by the library
 * Scripts are loaded once on start() and cached by SHA1
 */
export const LUA_SCRIPTS = {
  /**
   * Hash operations
   */
  HSET: `return redis.call('HSET', KEYS[1], KEYS[2], ARGV[1])`,
  HDEL: `return redis.call('HDEL', KEYS[1], KEYS[2])`,

  /**
   * Atomic hash field merge for partial updates
   * Electric sends partial updates (only changed columns) by default.
   * This script merges the partial update into the existing JSON.
   */
  HASH_UPDATE: `
    local current = redis.call('HGET', KEYS[1], KEYS[2])
    local parsed = {}
    if current then
      parsed = cjson.decode(current)
    end
    for k, v in pairs(cjson.decode(ARGV[1])) do
      parsed[k] = v
    end
    local updated = cjson.encode(parsed)
    return redis.call('HSET', KEYS[1], KEYS[2], updated)
  `,

  /**
   * Set operations
   */
  SADD: `return redis.call('SADD', KEYS[1], ARGV[1])`,
  SREM: `return redis.call('SREM', KEYS[1], ARGV[1])`,

  /**
   * Sorted set operations
   * ZADD updates score if member already exists (atomic)
   */
  ZADD: `return redis.call('ZADD', KEYS[1], ARGV[1], ARGV[2])`,
  ZREM: `return redis.call('ZREM', KEYS[1], ARGV[1])`,

  /**
   * Stream operations
   * XADD with variable number of field-value pairs
   */
  XADD: `
    local args = {}
    for i = 2, #ARGV do
      args[i-1] = ARGV[i]
    end
    return redis.call('XADD', KEYS[1], ARGV[1], unpack(args))
  `,
} as const

export type ScriptName = keyof typeof LUA_SCRIPTS

/**
 * SHA1 cache for loaded scripts
 */
export type ScriptShaCache = Record<ScriptName, string>

/**
 * Load all Lua scripts into Redis and return SHA1 cache
 */
export async function loadAllScripts(
  redis: RedisClientLike
): Promise<ScriptShaCache> {
  const cache = {} as ScriptShaCache

  for (const [name, script] of Object.entries(LUA_SCRIPTS)) {
    cache[name as ScriptName] = await loadScript(redis, script)
  }

  return cache
}
