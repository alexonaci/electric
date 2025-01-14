import React from 'react'
import { OmitKeyof, QueryClient } from '@tanstack/react-query'

import {
  PersistQueryClientOptions,
  PersistQueryClientProvider,
} from '@tanstack/react-query-persist-client'
import { createSyncStoragePersister } from '@tanstack/query-sync-storage-persister'

const defaultQueryClient = new QueryClient({
  defaultOptions: {
    queries: {
      gcTime: Infinity,
    },
  },
})

// Using offline persistence, see:
// https://tanstack.com/query/latest/docs/framework/react/guides/mutations#persisting-offline-mutations
const defaultPersister = createSyncStoragePersister({
  storage: window.localStorage,
})

export type ElectricTanstackQueryProviderProps = {
  children: React.ReactNode
  queryClient?: QueryClient
  persistOptions?: OmitKeyof<PersistQueryClientOptions, `queryClient`>
}

export const ElectricTanstackQueryProvider = ({
  children,
  queryClient = defaultQueryClient,
  persistOptions = { persister: defaultPersister },
}: ElectricTanstackQueryProviderProps) => {
  return (
    <PersistQueryClientProvider
      client={queryClient}
      persistOptions={persistOptions}
    >
      {children}
    </PersistQueryClientProvider>
  )
}
