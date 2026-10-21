// The latest release, read from GitHub when the site is built: the release
// workflow redeploys the site once a release is published, so the version and
// the sizes shown never fall behind the files the download links point to.

const REPO = 'naighes/scoutforall'

export const releasesUrl = `https://github.com/${REPO}/releases/latest`

export const downloadUrl = asset => `${releasesUrl}/download/${asset}`

let latest

export const latestRelease = () => latest ??= fetchLatestRelease()

const fetchLatestRelease = async () => {
  const token = process.env.GITHUB_TOKEN
  const headers = {
    Accept: 'application/vnd.github+json',
    ...(token && { Authorization: `Bearer ${token}` }),
  }
  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const { tag_name, assets } = await res.json()
    return {
      version: tag_name.replace(/^v/, ''),
      sizes: new Map(assets.map(({ name, size }) => [name, size])),
    }
  } catch (e) {
    console.warn(`cannot read the latest release: ${e.message}`)
    return null
  }
}

// The asset a download points to: `fallback` when the latest release does not
// carry `asset` yet, e.g. a new kind of package before its first release.
export const resolveAsset = (release, { asset, fallback }) =>
  release && fallback && !release.sizes.has(asset) ? fallback : asset
