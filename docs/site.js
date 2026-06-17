/* Shared site script for DepsGuard pages.
   Populates the GitHub star count, caching it in localStorage so a browser
   hits the GitHub API at most once per TTL across all pages (instead of once
   per page view). Degrades silently if the API or storage is unavailable. */
(function () {
  var el = document.getElementById('gh-star-count');
  if (!el) return;

  var KEY = 'dg-star-count';
  var TTL = 6 * 60 * 60 * 1000; // 6 hours

  function show(n) {
    el.textContent = n;
    el.classList.add('visible');
  }

  function cache(n) {
    try {
      localStorage.setItem(KEY, JSON.stringify({ n: n, t: Date.now() }));
    } catch (e) { /* storage unavailable */ }
  }

  try {
    var hit = JSON.parse(localStorage.getItem(KEY) || 'null');
    if (hit && hit.n != null && (Date.now() - hit.t) < TTL) {
      show(hit.n);
      return;
    }
  } catch (e) { /* ignore malformed cache */ }

  fetch('https://api.github.com/repos/arnica/depsguard')
    .then(function (r) { return r.json(); })
    .then(function (d) {
      if (d && d.stargazers_count != null) {
        show(d.stargazers_count);
        cache(d.stargazers_count);
      }
    })
    .catch(function () { /* offline or rate-limited; leave count hidden */ });
})();
