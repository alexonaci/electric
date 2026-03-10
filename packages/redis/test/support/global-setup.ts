import { Client } from 'pg'

const DATABASE_URL =
  process.env.DATABASE_URL ??
  `postgresql://postgres:password@localhost:54321/electric`
const ELECTRIC_URL = process.env.ELECTRIC_URL ?? `http://localhost:3000`
const REDIS_URL = process.env.REDIS_URL ?? `redis://localhost:6379`

// eslint-disable-next-line quotes -- eslint is acting dumb with enforce backtick quotes mode, and is trying to use it here where it's not allowed.
declare module 'vitest' {
  export interface ProvidedContext {
    electricUrl: string
    redisUrl: string
    testPgSchema: string
  }
}

function waitForElectric(url: string): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(
      () => reject(new Error(`Timed out waiting for Electric to be active`)),
      10000
    )

    const tryHealth = async (): Promise<void> => {
      try {
        const res = await fetch(`${url}/v1/health`)
        if (!res.ok) return tryHealth()
        const { status } = (await res.json()) as { status: string }
        if (status !== `active`) return tryHealth()
        clearTimeout(timeout)
        resolve()
      } catch {
        // Retry on connection errors
        await new Promise((r) => setTimeout(r, 100))
        return tryHealth()
      }
    }

    return tryHealth()
  })
}

function waitForRedis(url: string): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(
      () => reject(new Error(`Timed out waiting for Redis`)),
      10000
    )

    const tryConnect = async (): Promise<void> => {
      try {
        // Parse redis URL and try a simple connection
        const { createClient } = await import(`redis`)
        const client = createClient({ url })
        await client.connect()
        await client.ping()
        await client.quit()
        clearTimeout(timeout)
        resolve()
      } catch {
        await new Promise((r) => setTimeout(r, 100))
        return tryConnect()
      }
    }

    return tryConnect()
  })
}

export function makePgClient(options?: { options?: string }): Client {
  return new Client({
    connectionString: DATABASE_URL,
    ...options,
  })
}

/**
 * Global setup for the e2e test suite.
 * Validates that Electric and Redis are running, and creates a test schema.
 */
export default async function ({
  provide,
}: {
  provide: (key: string, value: unknown) => void
}) {
  console.log(`Waiting for Electric at ${ELECTRIC_URL}...`)
  await waitForElectric(ELECTRIC_URL)
  console.log(`Electric is ready`)

  console.log(`Waiting for Redis at ${REDIS_URL}...`)
  await waitForRedis(REDIS_URL)
  console.log(`Redis is ready`)

  const client = makePgClient()
  await client.connect()
  await client.query(`CREATE SCHEMA IF NOT EXISTS redis_test`)
  console.log(`Test schema created`)

  provide(`electricUrl`, ELECTRIC_URL)
  provide(`redisUrl`, REDIS_URL)
  provide(`testPgSchema`, `redis_test`)

  return async () => {
    await client.query(`DROP SCHEMA redis_test CASCADE`)
    await client.end()
    console.log(`Test schema dropped`)
  }
}
