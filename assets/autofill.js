// Injected into every page in rug's own script world, which the page can't
// see. Reports submitted logins to rug (which offers to save them) and
// provides rugFill() for filling a saved one.
(function () {
  var USER_TYPES = ['text', 'email', 'tel'];

  function visible(el) {
    return el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden';
  }

  function inputs(scope) {
    return Array.prototype.filter.call(scope.querySelectorAll('input'), visible);
  }

  // Verification codes (emailed, SMS, authenticator) are often password
  // fields too, but aren't worth saving.
  function isCode(input) {
    return input.autocomplete === 'one-time-code' ||
      /code|otp|token|2fa|mfa|totp|verif|security|pin/i.test(input.name + ' ' + input.id + ' ' + input.placeholder);
  }

  function passwordField(scope) {
    return inputs(scope).filter(function (i) { return i.type === 'password' && !isCode(i); })[0] || null;
  }

  // The username field: the text-like input just before the password, or on
  // a username-only page (the first step of a two-step login) the likeliest
  // looking one.
  function userField(scope, password) {
    var all = inputs(scope);
    if (password) {
      for (var i = all.indexOf(password) - 1; i >= 0; i--) {
        if (USER_TYPES.indexOf(all[i].type) >= 0) return all[i];
      }
      return null;
    }
    return all.filter(function (i) {
      return USER_TYPES.indexOf(i.type) >= 0 &&
        /user|login|email|account|identifier/i.test(i.type + ' ' + i.name + ' ' + i.id + ' ' + i.autocomplete);
    })[0] || null;
  }

  var lastSent = '';

  function capture(scope) {
    scope = scope || document;
    var password = passwordField(scope);
    var user = userField(scope, password);
    if (!password || !password.value) {
      // Remember the username for a password-only second step.
      if (user && user.value) sessionStorage.setItem('rug.username', user.value);
      return;
    }
    var username = user && user.value ? user.value : sessionStorage.getItem('rug.username') || '';
    // A password-only page right after a full login is a verification
    // step, not a second password.
    if (!user && username && sessionStorage.getItem('rug.sent') === username) return;
    var key = username + '\n' + password.value;
    if (key === lastSent) return;
    lastSent = key;
    sessionStorage.setItem('rug.sent', username);
    window.webkit.messageHandlers.rug.postMessage({ username: username, password: password.value });
  }

  // Sites log in with plain form submits, button clicks handled in script,
  // or Enter in a field; watch all three.
  document.addEventListener('submit', function (e) { capture(e.target); }, true);
  document.addEventListener('click', function (e) {
    var button = e.target.closest && e.target.closest('button, input[type=submit], [role=button]');
    if (button) capture(button.form);
  }, true);
  document.addEventListener('keydown', function (e) {
    if (e.key === 'Enter' && e.target.tagName === 'INPUT') capture(e.target.form);
  }, true);

  function set(input, value) {
    if (!input) return;
    input.focus();
    // Through the prototype's setter, so frameworks tracking the value
    // (React and friends) notice the change.
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
  }

  window.rugFill = function (username, password) {
    var field = passwordField(document);
    var scope = (field && field.form) || document;
    if (username) set(userField(scope, field), username);
    set(field, password);
    if (field) lastSent = username + '\n' + password;
  };
})();
