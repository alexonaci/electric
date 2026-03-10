import type { RedisClientLike, RedisMultiLike } from './types'

/**
 * Chunk an array into smaller arrays of specified size
 */
export function chunk<T>(array: T[], size: number): T[][] {
  const chunks: T[][] = []
  for (let i = 0; i < array.length; i += size) {
    chunks.push(array.slice(i, i + size))
  }
  return chunks
}

/**
 * Execute a Lua script via pipeline
 * This is the single universal helper for all Redis operations.
 * Using Lua scripts makes us completely client-agnostic.
 */
export function pipelineEvalSha(
  pipeline: RedisMultiLike,
  sha: string,
  keys: string[],
  args: (string | number)[]
): void {
  if (pipeline.evalSha) {
    // node-redis style
    pipeline.evalSha(sha, { keys, arguments: args.map(String) })
  } else if (pipeline.evalsha) {
    // ioredis/iovalkey style
    pipeline.evalsha(sha, keys.length, ...keys, ...args.map(String))
  } else {
    throw new Error(`Pipeline does not support evalSha or evalsha`)
  }
}

/**
 * Load a Lua script into Redis and return its SHA1 hash
 *
 * Handles naming differences between Redis client libraries:
 * - node-redis uses SCRIPT_LOAD
 * - ioredis uses scriptLoad
 * - iovalkey uses script('load', ...)
 */
export async function loadScript(
  redis: RedisClientLike,
  script: string
): Promise<string> {
  if (typeof redis.SCRIPT_LOAD === `function`) {
    return redis.SCRIPT_LOAD(script)
  }

  if (typeof redis.scriptLoad === `function`) {
    return redis.scriptLoad(script)
  }

  if (typeof redis.script === `function`) {
    return redis.script(`load`, script) as Promise<string>
  }

  throw new Error(
    `Redis client does not support script loading. ` +
      `Expected SCRIPT_LOAD (node-redis), scriptLoad (ioredis), or script (iovalkey) method.`
  )
}
