const repo = 'cveloy102/hl100-drop';
const viewer = document.querySelector('#viewer');
document.querySelectorAll('[data-viewer]').forEach(button => button.addEventListener('click', () => {
  const source = button.querySelector('img');
  const image = viewer.querySelector('img');
  image.src = source.src; image.alt = source.alt;
  viewer.querySelector('p').textContent = source.alt;
  viewer.showModal();
}));
viewer?.querySelector('.close').addEventListener('click', () => viewer.close());
viewer?.addEventListener('click', event => { if (event.target === viewer) viewer.close(); });
// A static fallback remains usable if GitHub is unavailable or rate-limited.
if (document.querySelector('[data-asset]')) {
  fetch(`https://api.github.com/repos/${repo}/releases/latest`, {headers: {Accept: 'application/vnd.github+json'}})
    .then(response => { if (!response.ok) throw new Error('GitHub unavailable'); return response.json(); })
    .then(release => {
      const version = release.tag_name?.replace(/^v/, '');
      if (!/^\d+\.\d+\.\d+$/.test(version) || release.draft || release.prerelease) return;
      const names = {windows: `HL100DropSetup-${version}.exe`, android: `HL100Drop-${version}.apk`, macos: `HL100Drop-${version}-universal.dmg`};
      const links = Object.fromEntries(Object.entries(names).map(([platform, name]) => [platform, release.assets?.find(asset => asset.name === name)?.browser_download_url]));
      if (!Object.values(links).every(url => typeof url === 'string' && url.startsWith(`https://github.com/${repo}/releases/download/`))) return;
      document.querySelectorAll('[data-version]').forEach(node => node.textContent = `V ${version}`);
      document.querySelectorAll('[data-asset]').forEach(node => node.href = links[node.dataset.asset]);
    }).catch(() => {});
}
