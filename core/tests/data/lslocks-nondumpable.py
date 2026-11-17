import ctypes
import importlib.util
import pathlib

source = pathlib.Path(__file__).parents[1] / "fixtures/util-linux/locks/keeper.py"
spec = importlib.util.spec_from_file_location("keeper", source)
keeper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(keeper)
original = keeper.ready_record


def ready(*args):
    if ctypes.CDLL(None).prctl(4, 0, 0, 0, 0) != 0:
        raise OSError("prctl PR_SET_DUMPABLE failed")
    original(*args)


keeper.ready_record = ready
keeper.main()
