"""Tests for QTY-03, QTY-04: Database utilities."""
import pytest
import threading
import sqlite3
from unittest.mock import patch

from src.core.db_utils import (
    ConnectionConfig,
    DatabaseConnection,
    db_transaction,
    atomic_operation,
    AtomicCounter,
    atomic_db_update,
    atomic_set_active_task,
)


class TestConnectionConfig:
    """Tests for ConnectionConfig."""

    def test_default_config(self):
        """Should have default values."""
        config = ConnectionConfig()
        assert config.path == "openforce.db"
        assert config.timeout == 30.0
        assert config.check_same_thread is False

    def test_custom_config(self):
        """Should accept custom values."""
        config = ConnectionConfig(
            path="custom.db",
            timeout=60.0,
            check_same_thread=True
        )
        assert config.path == "custom.db"
        assert config.timeout == 60.0
        assert config.check_same_thread is True


class TestDatabaseConnection:
    """Tests for DatabaseConnection."""

    def test_context_manager(self):
        """Should work as context manager."""
        config = ConnectionConfig(path=":memory:")
        db = DatabaseConnection(config)

        with db as conn:
            cursor = conn.cursor()
            cursor.execute("SELECT 1")
            result = cursor.fetchone()
            assert result == (1,)

    def test_singleton(self):
        """Should be singleton."""
        config = ConnectionConfig(path=":memory:")
        db1 = DatabaseConnection(config)
        db2 = DatabaseConnection(config)
        assert db1 is db2

    def test_rollback_on_exception(self):
        """Should rollback on exception."""
        config = ConnectionConfig(path=":memory:")
        db = DatabaseConnection(config)

        # Create table first
        with db as conn:
            conn.execute("CREATE TABLE test_rollback (id INTEGER PRIMARY KEY, value TEXT)")

        # Test rollback
        try:
            with db as conn:
                conn.execute("INSERT INTO test_rollback (id, value) VALUES (1, 'test')")
                raise ValueError("Test exception")
        except ValueError:
            pass

        # Verify data was rolled back
        with db as conn:
            cursor = conn.execute("SELECT COUNT(*) FROM test_rollback")
            count = cursor.fetchone()[0]
            assert count == 0

    def test_close(self):
        """Should close connection."""
        config = ConnectionConfig(path=":memory:")
        db = DatabaseConnection(config)

        with db as conn:
            conn.execute("SELECT 1")

        db.close()
        # After close, next access should create new connection
        with db as conn:
            cursor = conn.execute("SELECT 1")
            result = cursor.fetchone()
            assert result == (1,)


class TestDbTransaction:
    """Tests for db_transaction."""

    def test_transaction_commit(self):
        """Should commit on success."""
        with patch('src.core.db_utils.sqlite3.connect') as mock_connect:
            mock_conn = mock_connect.return_value
            mock_cursor = mock_conn.cursor.return_value

            with db_transaction() as conn:
                conn.cursor().execute("SELECT 1")

            mock_conn.commit.assert_called_once()
            mock_conn.close.assert_called_once()

    def test_transaction_rollback(self):
        """Should rollback on exception."""
        with patch('src.core.db_utils.sqlite3.connect') as mock_connect:
            mock_conn = mock_connect.return_value

            try:
                with db_transaction() as conn:
                    raise ValueError("Test exception")
            except ValueError:
                pass

            mock_conn.rollback.assert_called_once()
            mock_conn.close.assert_called_once()


class TestAtomicOperation:
    """Tests for atomic_operation."""

    def test_atomic_lock(self):
        """Should acquire and release lock."""
        lock = threading.Lock()

        with atomic_operation(lock) as acquired_lock:
            # Lock should be held inside context - try to acquire from another context
            # This should fail because the lock is already held
            can_acquire = lock.acquire(blocking=False)
            assert can_acquire is False

        # Lock should be released after context
        acquired = lock.acquire(blocking=False)
        assert acquired is True
        lock.release()

    def test_atomic_lock_default(self):
        """Should work with default lock."""
        with atomic_operation() as lock:
            # Just verify no exception
            pass

    def test_atomic_lock_thread_safety(self):
        """Should ensure thread safety."""
        results = []
        lock = threading.Lock()

        def append_in_order(value):
            with atomic_operation(lock):
                results.append(value)

        threads = [
            threading.Thread(target=append_in_order, args=(i,))
            for i in range(10)
        ]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        # All values should be present (order may vary)
        assert len(results) == 10
        assert set(results) == set(range(10))


class TestAtomicCounter:
    """Tests for AtomicCounter."""

    def test_increment(self):
        """Should increment value."""
        counter = AtomicCounter(0)
        assert counter.increment() == 1
        assert counter.increment() == 2
        assert counter.increment(5) == 7

    def test_decrement(self):
        """Should decrement value."""
        counter = AtomicCounter(10)
        assert counter.decrement() == 9
        assert counter.decrement(3) == 6

    def test_get_set(self):
        """Should get and set value."""
        counter = AtomicCounter(0)
        counter.set(42)
        assert counter.get() == 42

    def test_thread_safety(self):
        """Counter should be thread-safe."""
        counter = AtomicCounter(0)

        def increment_many():
            for _ in range(100):
                counter.increment()

        threads = [threading.Thread(target=increment_many) for _ in range(10)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        assert counter.get() == 1000

    def test_concurrent_increment_decrement(self):
        """Should handle concurrent increments and decrements."""
        counter = AtomicCounter(500)

        def increment():
            for _ in range(100):
                counter.increment()

        def decrement():
            for _ in range(100):
                counter.decrement()

        threads = []
        for _ in range(5):
            threads.append(threading.Thread(target=increment))
            threads.append(threading.Thread(target=decrement))

        for t in threads:
            t.start()
        for t in threads:
            t.join()

        # 500 + 500 increments - 500 decrements = 500
        assert counter.get() == 500


class TestAtomicDbUpdate:
    """Tests for atomic_db_update."""

    def test_update_existing_row(self):
        """Should update existing row."""
        with patch('src.core.db_utils.db_transaction') as mock_transaction:
            mock_conn = mock_transaction.return_value.__enter__.return_value
            mock_cursor = mock_conn.cursor.return_value
            mock_cursor.rowcount = 1

            result = atomic_db_update(
                table="tasks",
                key_column="task_id",
                key_value="task_1",
                update_column="status",
                update_value="completed"
            )

            assert result is True
            mock_cursor.execute.assert_called_once()

    def test_update_with_condition(self):
        """Should update with additional condition."""
        with patch('src.core.db_utils.db_transaction') as mock_transaction:
            mock_conn = mock_transaction.return_value.__enter__.return_value
            mock_cursor = mock_conn.cursor.return_value
            mock_cursor.rowcount = 1

            result = atomic_db_update(
                table="tasks",
                key_column="task_id",
                key_value="task_1",
                update_column="status",
                update_value="completed",
                condition="owner_user_id = 'user_1'"
            )

            assert result is True
            # Check that condition was added to SQL
            call_args = mock_cursor.execute.call_args
            assert "AND" in call_args[0][0]
            assert "owner_user_id = 'user_1'" in call_args[0][0]


class TestAtomicSetActiveTask:
    """Tests for atomic_set_active_task."""

    def test_set_task(self):
        """Should set active task."""
        with patch('src.core.db_utils.db_transaction') as mock_transaction:
            mock_conn = mock_transaction.return_value.__enter__.return_value
            mock_cursor = mock_conn.cursor.return_value

            result = atomic_set_active_task("user_1", "task_1")

            assert result is True
            mock_cursor.execute.assert_called_once()

    def test_clear_task(self):
        """Should clear active task."""
        with patch('src.core.db_utils.db_transaction') as mock_transaction:
            mock_conn = mock_transaction.return_value.__enter__.return_value
            mock_cursor = mock_conn.cursor.return_value

            result = atomic_set_active_task("user_1", None)

            assert result is True
            call_args = mock_cursor.execute.call_args
            assert "DELETE" in call_args[0][0]

    def test_upsert_behavior(self):
        """Should use upsert for setting task."""
        with patch('src.core.db_utils.db_transaction') as mock_transaction:
            mock_conn = mock_transaction.return_value.__enter__.return_value
            mock_cursor = mock_conn.cursor.return_value

            result = atomic_set_active_task("user_1", "task_1")

            assert result is True
            call_args = mock_cursor.execute.call_args
            sql = call_args[0][0]
            assert "INSERT" in sql
            assert "ON CONFLICT" in sql
            assert "DO UPDATE" in sql
