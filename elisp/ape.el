;; ape.el --- Kbd macro like functionality but AI-assisted

(require 'json)

(defgroup ape nil
  "AI-assisted editing macros."
  :group 'tools)

(defcustom ape-cli-command "/home/vineet/code/ape/target/debug/ape-cli"
  "Path to the AI macro CLI command."
  :type 'string
  :group 'ape)


;;; State

(defvar ape--macro-id nil
  "Non-nil when recording is active.")

(defvar ape--target-file nil
  "Non-nil when execute is called")


;;; Cache

(defvar ape--provider nil
  "Non-nil when provider is loaded once")


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
  "Open the AI macro log buffer."
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
             (error "AI macro CLI returned malformed JSON")))
        (ape--log 'error "CLI failed (exit %d): %s" exit-code stderr)
        (error "%s" (string-trim stderr))))))


;; Modeline

(defun ape--modeline-rec-status ()
  "Update modeline for visual cue to indicate recording is in progress"
  (setq global-mode-string
        (if ape--macro-id
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
  (condition-case err
      (let ((resp (ape--run-command "start" buffer-file-name)))
        (setq ape--macro-id (alist-get 'id resp))
        (ape--modeline-rec-status)
        (message "APE recording started")
        (ape--log 'error "Recording started: %s" ape--macro-id))
    (error (message "Failed to start recording: %s - %s" ape--macro-id (cadr err)))))

(defun ape-stop-macro ()
  (interactive)
  (if ape--macro-id
      (condition-case err
          (let ((resp (ape--run-command "stop" ape--macro-id)))
            (ape--modeline-rec-status)
            (message "APE recording stopped")
            (ape--log 'error "Recording stopped: %s" ape--macro-id))
        (error (message "Failed to stop recording: %s - %s" ape--macro-id (cadr err))))
    (error (message "No APE macro recording has been started"))))

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
    (message "AI macro running...")
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
                  (message "AI macro error: malformed response"))))
           (let ((stderr (with-temp-buffer
                           (insert-file-contents stderr-file)
                           (buffer-string))))
             (ape--log 'error "CLI failed (exit %d): %s" exit-code stderr)
             (message "AI macro failed: %s" (string-trim stderr))))
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
    (define-key map (kbd "C-c x e") #'ape-execute)
    (define-key map (kbd "C-c x v") #'ape-view-macro)
    map)
  "Keymap for `ape-mode'.")

(define-minor-mode ape-mode
  "Minor mode for AI-assisted macro recording."
  :lighter " Ape"
  :keymap ape-mode-map
  :global t)

(provide 'ape-mode)
