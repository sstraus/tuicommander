/** Track CTA intent on the official website. These events do not prove installation. */
(function () {
  if (!['tuicommander.com', 'www.tuicommander.com'].includes(window.location.hostname)) return;
  window.dataLayer = window.dataLayer || [];
  window.gtag = function () { window.dataLayer.push(arguments); };
  window.gtag('js', new Date());
  window.gtag('config', 'G-PCXPSMR9JZ');
  var script = document.createElement('script');
  script.async = true;
  script.src = 'https://www.googletagmanager.com/gtag/js?id=G-PCXPSMR9JZ';
  document.head.appendChild(script);
  document.addEventListener('click', function (event) {
    var setup = event.target.closest('[data-analytics-setup]');
    if (setup && ['homebrew', 'shell', 'powershell'].includes(setup.dataset.analyticsSetup)) {
      window.gtag('event', 'setup_copy_click', { method: setup.dataset.analyticsSetup });
      return;
    }
    var link = event.target.closest('a[href]');
    if (!link) return;
    var href = link.getAttribute('href');
    if (href === 'https://github.com/sstraus/tuicommander') {
      window.gtag('event', 'repository_click', {});
    } else if (href === '/download.html') {
      window.gtag('event', 'download_click', { destination: 'download_page' });
    } else if (/^https:\/\/github\.com\/sstraus\/tuicommander\/releases(?:\/|$)/.test(href)) {
      window.gtag('event', 'download_click', {
        destination: href.includes('/download/') ? 'asset' : 'release',
      });
    }
  });
})();
