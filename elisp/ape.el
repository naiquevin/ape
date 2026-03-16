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

(defvar ape--recording-id nil
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
        (if ape--recording-id
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
      (goto-char (point-min)))
    (pop-to-buffer buf)))

(defun ape-apply-diff ()
  "Apply the diff in the current review buffer."
  (interactive)
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
        (message "Failed to apply diff. Check *Messages* for details.")))))


(defun ape-reject-diff ()
  "Reject the diff and close the review buffer."
  (interactive)
  (setq ape--target-file nil)
  (quit-window t))

;;; Operations

(defun ape-start-macro ()
  (interactive)
  (ape--ensure-api-key)
  (condition-case err
      (let ((resp (ape--run-command "start" buffer-file-name)))
        (setq ape--recording-id (alist-get 'id resp))
        (ape--modeline-rec-status)
        (message "APE recording started")
        (ape--log 'error "Recording started: %s" ape--recording-id))
    (error (message "Failed to start recording: %s - %s" ape--recording-id (cadr err)))))

(defun ape-stop-macro ()
  (interactive)
  (condition-case err
      (let ((resp (ape--run-command "stop" ape--recording-id)))
        (ape--modeline-rec-status)
        (message "APE recording stopped")
        (ape--log 'error "Recording stopped: %s" ape--recording-id))
    (error (message "Failed to stop recording: %s - %s" ape--recording-id (cadr err)))))

(defun ape-execute (user-message)
  (interactive (list (read-string "Instructions (optional): ")))
  (let* ((args (if (string-empty-p user-message)
                   (list "execute" ape--recording-id buffer-file-name)
                 (list "execute" "--user-msg" user-message ape--recording-id buffer-file-name)))
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


;; Derived mode

(define-derived-mode ape-diff-mode diff-mode "AI-Diff"
  "Major mode for reviewing AI macro diffs.
Inherits from `diff-mode'. Use \\[ape-apply-diff] to apply,
\\[ape-reject-diff] to reject."
  (setq buffer-read-only t)
  (setq header-line-format
        (substitute-command-keys
         "AI Macro Diff  \\[ape-apply-diff] Apply  \\[ape-reject-diff] Reject  \\[diff-hunk-next]/\\[diff-hunk-prev] Navigate hunks")))


(define-key ape-diff-mode-map (kbd "a") #'ape-apply-diff)
(define-key ape-diff-mode-map (kbd "r") #'ape-reject-diff)
(define-key ape-diff-mode-map (kbd "q") #'ape-reject-diff)

;;; Global minor mode (for keybindings)

(defvar ape-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c x (") #'ape-start-macro)
    (define-key map (kbd "C-c x )") #'ape-stop-macro)
    (define-key map (kbd "C-c x e") #'ape-execute)
    map)
  "Keymap for `ape-mode'.")

(define-minor-mode ape-mode
  "Minor mode for AI-assisted macro recording."
  :lighter " Ape"
  :keymap ape-mode-map
  :global t)

(provide 'ape-mode)
