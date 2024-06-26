import anyTest, { TestFn } from 'ava'

import { makePgContext, cleanAndStopSatellite } from '../common'

import { getPgMatchingShadowEntries } from '../../support/satellite-helpers'
import { processTagsTests, ContextType } from '../process.tags'

let port = 5100

const test = anyTest as TestFn<ContextType>
test.beforeEach(async (t) => {
  const namespace = 'public'
  await makePgContext(t, port++, namespace)
  t.context.getMatchingShadowEntries = getPgMatchingShadowEntries
})
test.afterEach.always(cleanAndStopSatellite)

processTagsTests(test)
