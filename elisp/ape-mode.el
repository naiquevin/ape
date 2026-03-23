;; ape-mode.el --- Kbd macro like functionality but AI-assisted

;; Copyright (c) 2026 Vineet Naik <naikvin@gmail.com>
;; Author: Vineet Naik <naikvin@gmail.com>
;; URL: https://github.com/naiquevin/ape
;; Version: 0.1.0
;; Keywords: Kbd macros AI LLM

;; This program is *not* a part of emacs and is provided under the MIT
;; License (MIT) <http://opensource.org/licenses/MIT>
;;
;; Copyright (c) 2026 Vineet Naik <naikvin@gmail.com>
;;
;; Permission is hereby granted, free of charge, to any person
;; obtaining a copy of this software and associated documentation
;; files (the "Software"), to deal in the Software without
;; restriction, including without limitation the rights to use, copy,
;; modify, merge, publish, distribute, sublicense, and/or sell copies
;; of the Software, and to permit persons to whom the Software is
;; furnished to do so, subject to the following conditions:
;;
;; The above copyright notice and this permission notice shall be
;; included in all copies or substantial portions of the Software.
;;
;; THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
;; EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
;; MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
;; NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS
;; BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN
;; ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
;; CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
;; SOFTWARE.

;;; Commentary:
;;
;; Ape provides kbd-macro-like functionality but driven by AI. It's
;; useful for those cases where it's tedious to explain a change to
;; an LLM in plain english, whereas it's much easier to "show" an
;; example change and ask it to repeat it multiple times.
;;
;; Dependencies:
;;
;; This mode depends on the ape-cli command line tool. It's a hard
;; dependency as it does most of the heavy lifting, whereas the minor
;; mode is merely a thin wrapper to integrate with emacs. The CLI
;; tool is written in rust and can be built from source using cargo.
;;
;; Installation:
;;
;; 1. First build the ape-cli tool and copy it to a dir in `$PATH'
;;
;;      git clone git@github.com:naiquevin/ape.git
;;      cd ape
;;      cargo build -p ape-cli --release
;;      cp target/release/ape-cli ~/.local/bin/
;;
;; 2. Require the `elisp/ape-mode.el' file in your emacs config.
;;
;;    If you use `use-package`, you may add the following lines to your
;;    emacs config:
;;
;;      (use-package ape-mode
;;        :ensure nil
;;        :load-path "</path/to/ape-mode.el>"
;;        :init
;;        (ape-mode 1))
;;
;; Usage:
;;
;; It's a minor mode and works as follows:
;;
;;   1. User starts an APE macro recording with `C-c x (' (or M-x
;;   ape-start-macro)
;;
;;   2. User makes a change in the file
;;
;;   3. User stop the macro recording with `C-c x )' (or M-x
;;   ape-stop-macro)
;;
;;   4. User can now ask LLM to repeat the change with `C-c x e' (or
;;   M-x ape-execute). The diff obtained from LLM response is
;;   displayed in a buffer that derives from diff-mode, from where the
;;   user may accept or reject the change.
;;
;; Other functions provided by the mode: `ape-cancel-macro',
;; `ape-view-macro' etc. (See README.md for more details).

;;; Code:

(require 'json)

(defgroup ape nil
  "AI-assisted editing macros."
  :group 'tools)

(defcustom ape-cli-command "ape-cli"
  "Path to the APE CLI command."
  :type 'string
  :group 'ape)


;;; State

(defvar ape--rec-in-progress nil
  "Non-nil when recording is started and nil again when stopped. Value is
the macro id being recorded.")

(defvar ape--macro-id nil
  "Currently active macro. Non-nil when one is recorded or an existing one
is set to active")

(defvar ape--target-file nil
  "Non-nil when execute is called")


;;; Cache

(defvar ape--provider nil
  "Non-nil when provider is loaded for the first time (think of it as cache")


;;; Logging

(defvar ape--log-buffer-name "*APE Log*")

(defun ape--log (level fmt &rest args)
  "Append a log entry at LEVEL (debug/info/error) to the log buffer."
  (let ((buf (get-buffer-create ape--log-buffer-name))
        (msg (apply #'format fmt args))
        (timestamp (format-time-string "%Y-%m-%d %H:%M:%S")))
    (with-current-buffer buf
      (goto-char (point-max))
      (insert (format "[%s] [%s] %s\n" timestamp (upcase (symbol-name level)) msg)))))

(defun ape-show-log ()
  "Open the APE log buffer."
  (interactive)
  (pop-to-buffer ape--log-buffer-name))


;;; APE config and env vars
(defun ape--get-provider ()
  (if ape--provider
      ape--provider
    (let ((provider (alist-get 'provider (json-read-file (expand-file-name "~/.ape/config.json")))))
      (setq ape--provider provider)
      provider)))

(defun ape--ensure-api-key ()
  (let* ((provider (ape--get-provider))
         (envvar (cond ((string= provider "OpenAI") "OPENAI_API_KEY")
                       ((string= provider "Claude") "ANTHROPIC_API_KEY"))))
    (unless (getenv envvar)
      (let ((key (read-passwd (format "Set %s: " envvar))))
        (setenv envvar key)
        (clear-string key)))))


;;; Shelling out to the CLI

(defun ape--run-command (&rest args)
  "Run the CLI with ARGS. Returns parsed JSON or signals an error."
  (with-temp-buffer
    (let* ((stderr-file (make-temp-file "ape-stderr-"))
           (cmd (cons ape-cli-command args))
           ;; Specifying (list t stderr-file) as the destination to
           ;; send stdout to current buffer and stderr to stderr-file
           (exit-code (apply #'call-process (car cmd)
                             nil (list t stderr-file)
                             nil (cdr cmd)))
           (stdout (buffer-string))
           (stderr (with-temp-buffer
                     (insert-file-contents stderr-file)
                     (delete-file stderr-file)
                     (buffer-string))))
      (if (zerop exit-code)
          (condition-case _
              (json-parse-string stdout :object-type 'alist)
            (json-parse-error
             (ape--log 'error "Invalid JSON from CLI: %s" stdout)
             (error "APE CLI returned malformed JSON")))
        (ape--log 'error "CLI failed (exit %d): %s" exit-code stderr)
        (error "%s" (string-trim stderr))))))


;; Modeline

(defun ape--modeline-rec-status ()
  "Update modeline for visual cue to indicate recording is in progress"
  (setq global-mode-string
        (if ape--rec-in-progress
            '(:eval (propertize " ⏺REC" 'face '(:foreground "red" :weight bold)))
          ""))
  (force-mode-line-update t))


;;; Diff view buffer

(defun ape--show-diff (diff-text target-file)
  "Display DIFF-TEXT in a review buffer."
  (setq ape--target-file target-file)
  (let ((buf (get-buffer-create "*APE Diff*")))
    (with-current-buffer buf
      (let ((inhibit-read-only t))
        (erase-buffer)
        (insert diff-text))
      (ape-diff-mode)
      (setq-local ape-diff--context 'review)
      (ape-diff--set-header 'review)
      (goto-char (point-min)))
    (pop-to-buffer buf)))

(defun ape-apply-diff ()
  "Apply the diff in the current review buffer."
  (interactive)
  (if (eq ape-diff--context 'review)
      (let ((diff-text (buffer-string))
            (tmpfile (make-temp-file "ape-" nil ".patch")))
        (let ((coding-system-for-write 'utf-8))
          (write-region diff-text nil tmpfile))
        (let ((result (call-process "patch"
                                    nil nil nil
                                    ape--target-file "-i" tmpfile)))
          (delete-file tmpfile)
          (if (zerop result)
              (progn
                (message "Diff applied successfully.")
                (quit-window t)
                ;; revert the target buffer if it's open
                (when-let ((target-buffer (find-buffer-visiting ape--target-file)))
                  (with-current-buffer target-buffer
                    (revert-buffer t t t))))
            (message "Failed to apply diff. Check *Messages* for details."))))
    (user-error "Diff cannot be applied in display context")))

(defun ape-reject-diff ()
  "Reject the diff and close the review buffer."
  (interactive)
  (setq ape--target-file nil)
  (quit-window t))

(defun ape-activate-macro ()
  "Make the macro corresponding to the displayed diff the current macro"
  (interactive)
  (setq ape--macro-id ape-diff--displayed-macro-id)
  (quit-window t))

;;; Operations

(defun ape-start-macro ()
  (interactive)
  (ape--ensure-api-key)
  (if ape--rec-in-progress
      (error (message "Already recording an APE macro. Stop or discard it first."))
      (condition-case err
          (let ((resp (ape--run-command "start" buffer-file-name)))
            (setq ape--rec-in-progress (alist-get 'id resp))
            (ape--modeline-rec-status)
            (message "APE recording started")
            (ape--log 'info "Recording started: %s" ape--macro-id))
        (error (message "Failed to start recording: %s - %s" ape--macro-id (cadr err))))))

(defun ape-stop-macro ()
  (interactive)
  (if ape--rec-in-progress
      (condition-case err
          (let ((resp (ape--run-command "stop" ape--rec-in-progress)))
            (setq ape--macro-id ape--rec-in-progress)
            (message "APE recording stopped")
            (ape--log 'info "Recording stopped: %s" ape--rec-in-progress)
            (setq ape--rec-in-progress nil)
            (ape--modeline-rec-status))
        (error (message "Failed to stop recording: %s - %s" ape--rec-in-progress (cadr err))))
    (error (message "No APE macro recording has been started"))))

(defun ape-cancel-macro ()
  (interactive)
  (if ape--rec-in-progress
      (if (y-or-n-p "Discard APE macro recording?")
          (progn
            (message "Cancelling APE macro recording: %s." ape--rec-in-progress)
            (ape--run-command "cancel" ape--rec-in-progress)
            (ape--log 'info "Recording cancelled: %s" ape--rec-in-progress)
            (setq ape--rec-in-progress nil)
            (ape--modeline-rec-status))
        (message "No changes were made."))
    (message "No APE macro recording in progress.")))

(defun ape-execute (user-message)
  "Execute the macro"
  (interactive
   (progn
     ;; Ensure API key is set
     (ape--ensure-api-key)
     ;; Ensure a macro is selected/activated
     (when (null ape--macro-id)
       (setq ape--macro-id (ape--select-macro)))
     (list (read-string "Instructions (optional): "))))
  (let* ((args (if (string-empty-p user-message)
                   (list "execute" ape--macro-id buffer-file-name)
                 (list "execute" "--user-msg" user-message ape--macro-id buffer-file-name)))
         (stderr-file (make-temp-file "ape-stderr-"))
         (stdout-buf (generate-new-buffer " *ape-stdout*"))
         (cmd (mapconcat #'shell-quote-argument
                         (cons ape-cli-command args) " "))
         (proc (start-process-shell-command
                "ape-execute" stdout-buf
                (concat cmd " 2>" (shell-quote-argument stderr-file)))))
    ;; (ape--log 'debug "Command: %s" cmd)
    (set-process-coding-system proc 'utf-8 'utf-8)
    ;; Set stderr-file as the property on the process so that it's
    ;; available inside the closure through the process object that's
    ;; passed to it. Otherwise the stderr-file variable in the let*
    ;; binding won't be accessible inside the closure thanks to
    ;; dynamic binding (by default) in emacs.
    (process-put proc :stderr-file stderr-file)
    (process-put proc :target-file buffer-file-name)
    (ape--log 'info "Executing with message: %S" user-message)
    (message "APE macro running...")
    (set-process-sentinel
     proc
     (lambda (proc event)
       (let ((exit-code (process-exit-status proc))
             (stderr-file (process-get proc :stderr-file)))
         (if (zerop exit-code)
             (with-current-buffer (process-buffer proc)
               ;; (ape--log 'debug "Output = %S" (buffer-string))
               (condition-case _
                   (let* ((resp (json-parse-string (buffer-string) :object-type 'alist))
                          (diff (base64-decode-string (alist-get 'diff_b64 resp))))
                     (if (or (null diff) (string-empty-p diff))
                         (message "No changes suggested.")
                       (ape--show-diff diff (process-get proc :target-file))))
                 (json-parse-error
                  (ape--log 'error "Invalid JSON: %s" (buffer-string))
                  (message "APE error: malformed response"))))
           (let ((stderr (with-temp-buffer
                           (insert-file-contents stderr-file)
                           (buffer-string))))
             (ape--log 'error "CLI failed (exit %d): %s" exit-code stderr)
             (message "APE command failed: %s" (string-trim stderr))))
         (kill-buffer (process-buffer proc))
         (delete-file stderr-file))))))

(defun ape--select-macro ()
  "Allow user to select a macro from a list (completion prompt"
  (ape--ensure-api-key)
  (condition-case err
      (let* ((resp (ape--run-command "list"))
             (choices (mapcar
                       (lambda (m)
                         (let ((id (alist-get 'id m))
                               (name (alist-get 'name m)))
                           (if name
                               (cons name id)
                             (cons (concat id " "
                                           (file-name-nondirectory (alist-get 'file_path m))
                                           "<" (file-name-nondirectory (alist-get 'repo_path m)) ">")
                                   id))))
                       (alist-get 'macros resp)))
             (selected (completing-read "Select: " choices nil t))
             (selected-id (cdr (assoc selected choices))))
        selected-id)
    (error (message "Failed to list APE macros: %s" (cadr err)))))

(defun ape-view-macro ()
  "View the macro selected by user from completion prompt."
  (interactive)
  (condition-case err
      (let* ((selected-id (ape--select-macro))
             (changes-file (expand-file-name (file-name-concat "~/.ape" selected-id "changes.diff"))))
        (let ((buf (get-buffer-create "*APE macro*")))
          (with-current-buffer buf
            (let ((inhibit-read-only t)
                  (diff-text (with-temp-buffer
                               (insert-file-contents changes-file)
                               (buffer-string))))
              (erase-buffer)
              (insert diff-text))
            (ape-diff-mode)
            (setq-local ape-diff--context 'display)
            (setq-local ape-diff--displayed-macro-id selected-id)
            (ape-diff--set-header 'display)
            (goto-char (point-min)))
          (pop-to-buffer buf)))
    (error (message "Failed to display APE macro: %s" (cadr err)))))

;; TODO: Add function ape-rename-macro to rename the macro

;; Derived mode

(defvar-local ape-diff--context nil
  "Context for the diff buffer. Either `review` or `display`.")

(defvar-local ape-diff--displayed-macro-id nil
  "Macro/recording id that's displayed in the ape-diff buffer")

(defun ape-diff--set-header (context)
  (let ((ctx (or context ape-diff--context)))
    (ape--log 'debug "diff context = %s" ctx)
    (setq header-line-format
          (pcase ctx
            ('review
             (substitute-command-keys
              "Review diff  \\[ape-apply-diff] Apply  \\[ape-reject-diff] Reject  \\[diff-hunk-next]/\\[diff-hunk-prev] Navigate hunks"))
            ('display
             (substitute-command-keys
              "Macro diff \\[ape-activate-macro] Select current  \\[quit-window] Close  \\[diff-hunk-next]/\\[diff-hunk-prev] Navigate"))
            (_
             "APE diff")))))

(define-derived-mode ape-diff-mode diff-mode "AI-Diff"
  "Major mode for displaying or reviewing diffs.
Inherits from `diff-mode'."
  (setq buffer-read-only t))


(define-key ape-diff-mode-map (kbd "a") #'ape-apply-diff)
(define-key ape-diff-mode-map (kbd "r") #'ape-reject-diff)
(define-key ape-diff-mode-map (kbd "q") #'ape-reject-diff)
(define-key ape-diff-mode-map (kbd "c") #'ape-activate-macro)

;;; Global minor mode (for keybindings)

(defvar ape-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c x (") #'ape-start-macro)
    (define-key map (kbd "C-c x )") #'ape-stop-macro)
    (define-key map (kbd "C-c x k") #'ape-cancel-macro)
    (define-key map (kbd "C-c x e") #'ape-execute)
    (define-key map (kbd "C-c x v") #'ape-view-macro)
    map)
  "Keymap for `ape-mode'.")

;;;###autoload
(define-minor-mode ape-mode
  "Minor mode for AI-assisted macro recording."
  :lighter " Ape"
  :keymap ape-mode-map
  :global t)


(provide 'ape-mode)
