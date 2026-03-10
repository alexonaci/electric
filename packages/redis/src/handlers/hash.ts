import type { RedisMultiLike } from '../types'
import type { ScriptShaCache } from '../lua-scripts'
import { pipelineEvalSha } from '../utils'

type Operation = `insert` | `update` | `delete`

/**
 * Hash handler using Lua scripts
 */
export function handleHash(
  pipeline: RedisMultiLike,
  operation: Operation,
  key: string,
  data: Record<string, unknown> | null,
  shapeName: string,
  scripts: ScriptShaCache
): void {
  switch (operation) {
    case `delete`:
      pipelineEvalSha(pipeline, scripts.HDEL, [shapeName, key], [])
      break

    case `insert`:
      if (data !== null) {
        pipelineEvalSha(
          pipeline,
          scripts.HSET,
          [shapeName, key],
          [JSON.stringify(data)]
        )
      }
      break

    case `update`:
      if (data !== null) {
        // Use atomic merge script for partial updates
        pipelineEvalSha(
          pipeline,
          scripts.HASH_UPDATE,
          [shapeName, key],
          [JSON.stringify(data)]
        )
      }
      break
  }
}
