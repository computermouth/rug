// Enable "Apply Changes" only when the form differs from what was loaded,
// and show the custom URL field only for the custom engine.
(function () {
  var form = document.getElementById('settings-form');
  var apply = document.getElementById('apply');
  var customUrl = document.getElementById('custom-url');

  function serialize() {
    return new URLSearchParams(new FormData(form)).toString();
  }

  var initial = serialize();

  function refresh() {
    var dirty = serialize() !== initial;
    apply.disabled = !dirty;
    apply.classList.toggle('dirty', dirty);
    customUrl.hidden = form.elements.engine.value !== 'custom';
  }

  var sync = document.getElementById('sync');
  sync.addEventListener('click', function () { sync.textContent = 'Syncing…'; });

  form.addEventListener('input', refresh);
  form.addEventListener('change', refresh);
})();
