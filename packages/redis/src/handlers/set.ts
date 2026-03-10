import type { RedisMultiLike } from '../types'
import type { ScriptShaCache } from '../lua-scripts'
import { pipelineEvalSha } from '../utils'

type Operation = `insert` | `update` | `delete`

/**
 * Redis Set handler using Lua scripts
 *
 * Stores each record's key as a member in a Redis Set.
 * Sets store unique members for efficient membership checks.
 */
export function handleSet(
  pipeline: RedisMultiLike,
  operation: Operation,
  key: string,
  shapeName: string,
  scripts: ScriptShaCache
): void {
  switch (operation) {
    case `delete`:
      pipelineEvalSha(pipeline, scripts.SREM, [shapeName], [key])
      break

    case `insert`:
      pipelineEvalSha(pipeline, scripts.SADD, [shapeName], [key])
      break

    case `update`:
      // For sets, update is a no-op since we only store keys
      // The key itself doesn't change on update
      break
  }
}
