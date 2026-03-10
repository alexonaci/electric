import type { RedisMultiLike, InternalShapeConfig } from '../types'
import type { ScriptShaCache } from '../lua-scripts'
import { pipelineEvalSha } from '../utils'

type Operation = `insert` | `update` | `delete`

/**
 * Extract a numeric score from data based on score config
 * THROWS if score field is missing or non-numeric - no fallbacks
 */
function extractScore(
  data: Record<string, unknown> | null,
  scoreField: string,
  key: string
): number {
  if (!data) {
    throw new Error(`Cannot extract score for key '${key}': data is null`)
  }

  const value = data[scoreField]

  if (value === undefined || value === null) {
    throw new Error(
      `Score field '${scoreField}' is missing in data for key '${key}'. ` +
        `sorted_set requires a valid numeric score field.`
    )
  }

  if (typeof value === `number`) {
    if (!Number.isFinite(value)) {
      throw new Error(
        `Score field '${scoreField}' has non-finite value (${value}) for key '${key}'`
      )
    }
    return value
  }

  if (typeof value === `string`) {
    const num = parseFloat(value)
    if (!isNaN(num) && Number.isFinite(num)) {
      return num
    }
  }

  throw new Error(
    `Score field '${scoreField}' must be numeric, got ${typeof value} (${value}) for key '${key}'`
  )
}

/**
 * Redis Sorted Set handler using Lua scripts
 *
 * Stores each record's key (ID) as the member in a Redis Sorted Set with its score.
 * The member value is the record's unique key/ID, NOT the full JSON data.
 *
 * This follows Redis idioms:
 * - ZADD with same member updates the score (no duplicates)
 * - ZREM by key works efficiently
 * - Use ZRANGE to get ordered IDs, then fetch full data from a hash if needed
 */
export function handleSortedSet(
  pipeline: RedisMultiLike,
  operation: Operation,
  key: string,
  data: Record<string, unknown> | null,
  shapeName: string,
  scripts: ScriptShaCache,
  config?: InternalShapeConfig
): void {
  if (!config?.score) {
    throw new Error(
      `sorted_set handler requires 'score' field in config for shape '${shapeName}'`
    )
  }

  switch (operation) {
    case `delete`:
      // Remove by key (the member value is the key/ID)
      pipelineEvalSha(pipeline, scripts.ZREM, [shapeName], [key])
      break

    case `insert`:
    case `update`: {
      // ZADD with the same member updates the score (Redis behavior)
      // So insert and update are identical operations
      const score = extractScore(data, config.score, key)
      pipelineEvalSha(pipeline, scripts.ZADD, [shapeName], [score, key])
      break
    }
  }
}
