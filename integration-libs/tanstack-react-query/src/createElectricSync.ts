
import {
  useMutation,
  useMutationState,
  useQueryClient,
  UseMutationOptions,
  MutationKey,
  QueryClient,
} from '@tanstack/react-query'
import { getShapeStream, useShape } from '@electric-sql/react'
import { matchStream } from './match-stream'
import { ChangeMessage, Row, ShapeStreamOptions } from '@electric-sql/client'

export interface ElectricMutationOptions<
  TVariables,
  TData = unknown,
  TError = unknown,
  TContext = unknown,
> extends UseMutationOptions<TData, TError, TVariables, TContext> {
  // > extends Omit<UseMutationOptions<TData, TError, TVariables, TContext>, "mutationKey" | "mutationFn"> {
  //   mutationKey?: MutationKey
  //   mutationFn: (args: TVariables) => Promise<TData>

  matchFn?: ({
    operationType,
    message,
  }: {
    operationType: string
    message: ChangeMessage<Row<unknown>>
  }) => boolean
}

type KeyAsString<T> = {
  [K in keyof T]: T[K] extends string ? K : never
}[keyof T]

export interface ShapeConfig<T> {
  options: ShapeStreamOptions
  id: KeyAsString<T>
}

export interface ElectricSyncConfig<T extends Row<unknown>> {
  queryClient?: QueryClient
  shape: ShapeConfig<T>
  insertMutation?: ElectricMutationOptions<T>
  updateMutation?: ElectricMutationOptions<T>
  deleteMutation?: ElectricMutationOptions<T>
  deleteManyMutation?: ElectricMutationOptions<number>
}

export function useElectricSync<T extends Row<unknown>>(
  config: ElectricSyncConfig<T>
) {
  const queryClient = config.queryClient ?? useQueryClient()

  const {
    shape: { id, options },
    insertMutation,
    deleteMutation,
    updateMutation,
    deleteManyMutation,
  } = config

  // same shape read as before
  const { data: shapeData } = useShape<T>(options)

  // Helper to remove certain mutation keys from the cache
  function removeMutationFromCache(mutationKeys: MutationKey[]): void {
    for (const mk of mutationKeys) {
      const matches = queryClient
        .getMutationCache()
        .findAll({ mutationKey: mk })
      matches.forEach((m) => {
        queryClient.getMutationCache().remove(m)
      })
    }
  }

  const insertMut = useInsertMutation<T>(insertMutation, options, id)


  const updateMut = useMutation({
    mutationKey: updateMutation?.mutationKey,
    mutationFn: async (row: T) => {
      if (!updateMutation?.mutationFn) return

      const stream = getShapeStream<T>(options)
      const findUpdatePromise = matchStream({
        stream,
        operations: [`update`],
        matchFn:
          updateMutation.matchFn ??
          (({ message }) => {
            return message.value[id] === row[id]
          }),
      })

      const fetchPromise = updateMutation.mutationFn(row)
      return await Promise.all([findUpdatePromise, fetchPromise])
    },
    onMutate: (optimisticValue: T) => {
      if (insertMutation?.onMutate) {
        insertMutation?.onMutate(optimisticValue)
      }
      return optimisticValue
    },
    ...updateMutation
  })


  const deleteManyMut = useMutation({
    mutationKey: deleteManyMutation?.mutationKey,
    mutationFn: async (count: number) => {
      if (!deleteManyMutation?.mutationFn) return

      const stream = getShapeStream<T>(options)
      const findUpdatePromise =
        count > 0
          ? matchStream({
            stream,
            operations: [`delete`],
            matchFn: deleteManyMutation.matchFn ?? (() => true),
          })
          : Promise.resolve()

      const fetchPromise = deleteManyMutation.mutationFn(count)
      return await Promise.all([findUpdatePromise, fetchPromise])
    },
    onMutate: (variables) => {
      if (!deleteManyMutation?.mutationFn) return

      if (updateMutation && insertMutation) {
        removeMutationFromCache([
          insertMutation.mutationKey!,
          updateMutation.mutationKey!,
        ])
      }

      if (deleteManyMutation.onMutate) {
        return deleteManyMutation.onMutate(variables)
      }
    },
    ...deleteManyMutation
  })


  const deleteMut = useMutation({
    mutationKey: deleteMutation?.mutationKey,
    mutationFn: async (row: T) => {
      if (!deleteMutation?.mutationFn) return

      const stream = getShapeStream<T>(options)
      const findUpdatePromise = matchStream({
        stream,
        operations: [`delete`],
        matchFn:
          deleteMutation.matchFn ??
          (({ message }) => {
            return message.value[id] === row[id]
          }),
      })

      const fetchPromise = deleteMutation.mutationFn(row)
      return await Promise.all([findUpdatePromise, fetchPromise])
    },
    onMutate: (variables) => {
      if (!deleteMutation) return
      if (updateMutation && insertMutation) {
        removeMutationFromCache([
          insertMutation.mutationKey!,
          updateMutation.mutationKey!,
        ])
      }
      if (deleteMutation.onMutate) {
        return deleteMutation.onMutate(variables)
      }
    },
    ...deleteMutation
  })

  const submissions: T[] = useMutationState({
    filters: { status: `pending`, mutationKey: insertMutation?.mutationKey },
    select: (mutation) => mutation.state.context as T,
  }).filter((item) => item !== undefined)

  // Merge data from shape & optimistic data from fetchers. This removes
  // possible duplicates as there's a potential race condition where
  // useShape updates from the stream slightly before the action has finished.
  const mergedData = new Map<string, T>()
  if (!deleteManyMut.isPending) {
    shapeData.concat(submissions).forEach((row) => {
      const rowKey = row[id] as string

      mergedData.set(rowKey, {
        ...mergedData.get(rowKey),
        ...row,
      })
    })
  } else {
    submissions.forEach((row) => {
      mergedData.set(row[id] as string, row)
    })
  }

  const data = [...mergedData.values()]

  return {
    data,
    insert: insertMut,
    deleteMany: deleteManyMut,
    update: updateMut,
    delete: deleteMut,
  }
}

function useInsertMutation<T extends Row<unknown>>(insertMutation: ElectricMutationOptions<T, unknown, unknown, unknown> | undefined, options: ShapeStreamOptions<never>, id: KeyAsString<T>) {

  if (!insertMutation) return

  return useMutation({
    mutationKey: insertMutation.mutationKey,
    mutationFn: async (newRow: T) => {
      const stream = getShapeStream(options)

      const findUpdatePromise = matchStream({
        stream,
        operations: [`insert`],
        matchFn: insertMutation.matchFn ??
          (({ message }) => {
            return (message.value as T)[id] === newRow[id]
          }),
      })

      const fetchPromise = insertMutation.mutationFn && insertMutation.mutationFn(newRow)

      return await Promise.all([findUpdatePromise, fetchPromise])
    },
    onMutate: (optimisticValue: T) => {
      if (insertMutation?.onMutate) {
        insertMutation?.onMutate(optimisticValue)
      }
      return optimisticValue
    },
    ...insertMutation
  })
}

