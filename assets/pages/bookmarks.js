// Buttons with a data-bookmark attribute drive the bookmark dialog:
//   add / add-folder  (data-parent: folder to put it in)
//   edit              (data-id, data-title, data-url, data-parent, data-folder)
//   delete            (data-id, data-title, data-folder)
(function () {
  var dialog = document.getElementById('bookmark-dialog');
  if (!dialog) return;
  var form = dialog.querySelector('form');
  var f = form.elements;

  function open(opts) {
    dialog.querySelector('.dialog-title').textContent = opts.heading;
    f.action.value = opts.action;
    f.kind.value = opts.folder ? 'folder' : 'link';
    f.id.value = opts.id || '';
    f.title.value = opts.title || '';
    f.url.value = opts.url || '';
    f.parent.value = opts.parent || '';
    form.querySelector('.url-field').hidden = opts.folder;
    f.url.required = !opts.folder;
    // A folder can't move into itself or its own subfolders.
    Array.prototype.forEach.call(f.parent.options, function (o) {
      o.disabled = !!(opts.folder && opts.id) && o.dataset.path.split(' ').indexOf(opts.id) >= 0;
    });
    dialog.showModal();
    (opts.folder || opts.url ? f.title : f.url).focus();
  }

  document.addEventListener('click', function (e) {
    var button = e.target.closest('[data-bookmark]');
    if (!button) return;
    e.preventDefault();
    var d = button.dataset;
    var folder = 'folder' in d;
    switch (d.bookmark) {
      case 'add':
        open({ heading: 'Add Bookmark', action: 'add', parent: d.parent });
        break;
      case 'add-folder':
        open({ heading: 'New Folder', action: 'add', folder: true, parent: d.parent });
        break;
      case 'edit':
        open({
          heading: folder ? 'Edit Folder' : 'Edit Bookmark', action: 'edit', folder: folder,
          id: d.id, title: d.title, url: d.url, parent: d.parent,
        });
        break;
      case 'delete':
        var question = folder
          ? 'Delete the folder "' + d.title + '" and everything in it?'
          : 'Delete "' + d.title + '"?';
        if (confirm(question)) {
          location.href = 'rug://bookmarks?action=delete&id=' + d.id + '&return=' + f['return'].value;
        }
        break;
    }
  });

  form.querySelector('.cancel').addEventListener('click', function () {
    dialog.close();
  });
})();

// The bookmark tree on rug://bookmarks: remembers collapsed folders, and
// drag and drop to reorder or move into folders. A drop reloads the page
// through ?action=move.
(function () {
  var tree = document.querySelector('.tree');
  if (!tree) return;

  var KEY = 'rug.closedFolders';
  var closed = [];
  try { closed = JSON.parse(localStorage.getItem(KEY)) || []; } catch (e) {}
  tree.querySelectorAll('.folder > details').forEach(function (details) {
    var id = details.parentNode.dataset.id;
    if (closed.indexOf(id) >= 0) details.open = false;
    details.addEventListener('toggle', function () {
      closed = closed.filter(function (c) { return c !== id; });
      if (!details.open) closed.push(id);
      try { localStorage.setItem(KEY, JSON.stringify(closed)); } catch (e) {}
    });
  });

  var dragged = null;
  var target = null;  // the <li> being dropped on, or the tree itself
  var where = null;   // 'before' | 'after' | 'into' | 'end'

  function clearTarget() {
    if (target) target.classList.remove('drop-before', 'drop-after', 'drop-into', 'drop-end');
    target = where = null;
  }

  tree.addEventListener('dragstart', function (e) {
    // dragend doesn't always arrive after a refused drop; tidy up here too.
    tree.querySelectorAll('.dragging').forEach(function (li) { li.classList.remove('dragging'); });
    clearTarget();
    dragged = e.target.closest('li[data-id]');
    if (!dragged) return;
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', dragged.dataset.id);
    dragged.classList.add('dragging');
  });

  tree.addEventListener('dragend', function () {
    if (dragged) dragged.classList.remove('dragging');
    dragged = null;
    clearTarget();
  });

  tree.addEventListener('dragover', function (e) {
    if (!dragged) return;
    clearTarget();
    if (e.target === tree) {
      // Empty space below everything: end of the top level.
      e.preventDefault();
      target = tree;
      where = 'end';
      tree.classList.add('drop-end');
      return;
    }
    var row = e.target.closest('.link, .folder > details > summary');
    var li = row && row.closest('li[data-id]');
    if (!li || dragged.contains(li)) return;  // not a row, or itself / its own contents
    e.preventDefault();
    var box = row.getBoundingClientRect();
    var y = (e.clientY - box.top) / box.height;
    if (li.classList.contains('folder')) where = y < 0.3 ? 'before' : 'into';
    else where = y < 0.5 ? 'before' : 'after';
    target = li;
    li.classList.add('drop-' + where);
  });

  tree.addEventListener('drop', function (e) {
    e.preventDefault();
    if (!dragged || !where) return;
    var parent = '';
    var before = '';
    if (where === 'into') {
      parent = target.dataset.id;
    } else if (where !== 'end') {
      parent = target.dataset.parent;
      var next = where === 'before' ? target : target.nextElementSibling;
      if (next === dragged) next = next.nextElementSibling;
      before = next ? next.dataset.id : '';
    }
    location.replace('rug://bookmarks?action=move&id=' + dragged.dataset.id +
      '&parent=' + parent + '&before=' + before + '&return=bookmarks');
  });
})();
