/**
 * Generates MDX parameter documentation from extract-sdk-types JSON output.
 *
 * Usage:
 *   bun scripts/sdk/generate-param-docs.ts <module> <function> [--wagmi]
 *
 * Examples:
 *   bun scripts/sdk/generate-param-docs.ts dex createPair
 *   bun scripts/sdk/generate-param-docs.ts dex createPair --wagmi
 *
 * Output:
 *   viem:  snippets/sdk/params/<module>.<function>.mdx
 *   wagmi: snippets/sdk/params/<module>.<function>.wagmi.mdx
 */

import * as fs from 'node:fs'
import * as path from 'node:path'

const args = process.argv.slice(2)
const isWagmi = args.includes('--wagmi')
const [moduleName, functionName] = args.filter((a) => !a.startsWith('--'))

if (!moduleName || !functionName) {
  console.error(
    'Usage: bun scripts/sdk/generate-param-docs.ts <module> <function> [--wagmi]',
  )
  console.error('Example: bun scripts/sdk/generate-param-docs.ts dex createPair')
  process.exit(1)
}

interface ParamInfo {
  name: string
  type: string
  optional: boolean
  description?: string
}

interface TypeInfo {
  module: string
  function: string
  actionType: 'read' | 'write' | 'watch'
  hasSyncVariant: boolean
  parameters: ParamInfo[]
}

// Read the JSON file
const inputPath = path.join(
  process.cwd(),
  'snippets/sdk/types',
  `${moduleName}.${functionName}.json`,
)

if (!fs.existsSync(inputPath)) {
  console.error(`Input file not found: ${inputPath}`)
  console.error(
    `Run extract-sdk-types first: bun scripts/extract-sdk-types.ts ${moduleName} ${functionName}`,
  )
  process.exit(1)
}

const typeInfo: TypeInfo = JSON.parse(fs.readFileSync(inputPath, 'utf-8'))

// Enhanced descriptions for common parameters
const ENHANCED_DESCRIPTIONS: Record<
  string,
  { viem?: string; wagmi?: string; common?: string }
> = {
  account: {
    viem: 'Account that will be used to send the transaction.',
    wagmi:
      'Account that will be used to send the transaction. Defaults to connected Wagmi account.',
  },
  feeToken: {
    common:
      'Fee token for the transaction. Can be a TIP-20 token address or ID.',
  },
  feePayer: {
    common:
      'Fee payer for the transaction. Can be a [Viem Account](https://viem.sh/docs/accounts/local/privateKeyToAccount), or `true` if a [Fee Payer Service](/sdk/typescript/server/handler.feePayer) will be used.',
  },
  gas: {
    common: 'Gas limit for the transaction.',
  },
  maxFeePerGas: {
    common: 'Max fee per gas for the transaction.',
  },
  maxPriorityFeePerGas: {
    common: 'Max priority fee per gas for the transaction.',
  },
  nonce: {
    common: 'Nonce for the transaction.',
  },
  nonceKey: {
    common:
      "Nonce key for the transaction. Use `'random'` to generate a random nonce key.",
  },
  validBefore: {
    common: 'Unix timestamp before which the transaction must be included.',
  },
  validAfter: {
    common: 'Unix timestamp after which the transaction can be included.',
  },
  throwOnReceiptRevert: {
    common:
      'Whether to throw an error if the transaction receipt indicates a revert. Only applicable to `*Sync` actions.',
  },
}

// Generate MDX content
function generateParamDoc(param: ParamInfo): string {
  const optionalSuffix = param.optional ? ' (optional)' : ''
  const lines = [
    `### ${param.name}${optionalSuffix}`,
    '',
    `- **Type:** \`${param.type}\``,
    '',
  ]

  // Get enhanced description if available
  const enhanced = ENHANCED_DESCRIPTIONS[param.name]
  let description = param.description
  if (enhanced) {
    if (isWagmi && enhanced.wagmi) {
      description = enhanced.wagmi
    } else if (!isWagmi && enhanced.viem) {
      description = enhanced.viem
    } else if (enhanced.common) {
      description = enhanced.common
    }
  }

  if (description) {
    lines.push(description)
    lines.push('')
  }

  return lines.join('\n')
}

const mdxContent = typeInfo.parameters.map(generateParamDoc).join('\n')

// Write the MDX file
const outputDir = path.join(process.cwd(), 'snippets/sdk/params')
fs.mkdirSync(outputDir, { recursive: true })

const suffix = isWagmi ? '.wagmi' : ''
const outputPath = path.join(
  outputDir,
  `${moduleName}.${functionName}${suffix}.mdx`,
)
fs.writeFileSync(outputPath, mdxContent)

const variant = isWagmi ? 'wagmi' : 'viem'
console.log(
  `✓ Generated ${variant} parameter docs for ${moduleName}.${functionName}`,
)
console.log(`  Output: ${outputPath}`)
