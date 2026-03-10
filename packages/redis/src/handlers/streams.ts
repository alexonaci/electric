import type { RedisMultiLike } from '../types'
import type { ScriptShaCache } from '../lua-scripts'
import { pipelineEvalSha } from '../utils'

type Operation = `insert` | `update` | `delete`

/**
 * Redis Streams handler using Lua scripts
 *
 * Stores Electric changes as Redis Stream entries. Each entry contains:
 * - Auto-generated timestamp ID (by Redis)
 * - The record's key
 * - The operation type (insert/update/delete)
 * - The record data as individual fields
 *
 * This follows the Redis Streams pattern from the docs:
 * https://redis.io/docs/data-types/streams/
 */
export function handleStream(
  pipeline: RedisMultiLike,
  operation: Operation,
  key: string,
  data: Record<string, unknown> | null,
  shapeName: string,
  scripts: ScriptShaCache
): void {
  // Build field-value pairs for XADD
  // Always include operation and key for identification
  const fields: string[] = [`operation`, operation, `key`, key]

  // Add data fields directly (not wrapped in a "data" field)
  if (data !== null) {
    for (const [field, value] of Object.entries(data)) {
      fields.push(
        field,
        typeof value === `object` ? JSON.stringify(value) : String(value)
      )
    }
  }

  // XADD with auto-generated ID (*)
  pipelineEvalSha(pipeline, scripts.XADD, [shapeName], [`*`, ...fields])
}
