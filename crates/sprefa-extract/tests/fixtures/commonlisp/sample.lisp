(defpackage #:patrol
  (:use #:cl)
  (:export #:reached-p #:step))

(in-package #:patrol)

(defparameter *speed* 120.0)
(defvar *state* :idle)

(defstruct waypoint
  (name nil :type string)
  (x 0.0 :type single-float))

(defun reached-p (waypoint)
  "True when WAYPOINT names the middle of the route."
  (string= (waypoint-name waypoint) "mid"))

(defun step (delta waypoints)
  (loop for waypoint in waypoints
        when (reached-p waypoint)
          collect (list waypoint (* delta *speed*))))

(defmacro with-state ((state) &body body)
  `(let ((*state* ,state))
     ,@body))

(defmethod describe-state ((state (eql :idle)))
  (format nil "idle at ~a" *speed*))

(defclass patrol ()
  ((waypoints :initarg :waypoints :accessor patrol-waypoints)
   (speed :initform *speed* :accessor patrol-speed)))

(defgeneric advance (patrol delta))

(defmethod advance ((patrol patrol) delta)
  (mapcar (lambda (waypoint) (list waypoint delta))
          (patrol-waypoints patrol)))

(format t "~&~a~%" (with-state (:walking) (describe-state *state*)))