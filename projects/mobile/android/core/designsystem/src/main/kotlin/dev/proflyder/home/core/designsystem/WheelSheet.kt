package dev.proflyder.home.core.designsystem

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.NumberPicker
import android.widget.TextView
import androidx.fragment.app.FragmentManager
import androidx.fragment.app.viewModels
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.bottomsheet.BottomSheetDialogFragment
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

class WheelSelection(private val savedState: SavedStateHandle) : ViewModel() {
    val index: StateFlow<Int> = savedState.getStateFlow(KEY_INDEX, savedState.get<Int>(ARG_INITIAL) ?: 0)

    fun select(index: Int) {
        savedState[KEY_INDEX] = index
    }

    internal companion object {
        const val ARG_INITIAL = "ds_wheel_initial"
        const val KEY_INDEX = "ds_wheel_index"
    }
}

class WheelSheet : BottomSheetDialogFragment() {
    private val selection: WheelSelection by viewModels()
    private val args: Bundle get() = requireArguments()

    override fun onCreateView(inflater: LayoutInflater, container: ViewGroup?, savedInstanceState: Bundle?): View =
        inflater.inflate(R.layout.ds_sheet_wheel, container, false)

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        view.findViewById<TextView>(R.id.ds_sheet_title).text = args.getCharSequence(ARG_TITLE)
        val wheel = view.findViewById<NumberPicker>(R.id.ds_sheet_wheel).apply(::setUpWheel)
        view.findViewById<View>(R.id.ds_sheet_cancel).setOnClickListener { dismiss() }
        view.findViewById<View>(R.id.ds_sheet_done).setOnClickListener { confirm() }
        viewLifecycleOwner.lifecycleScope.launch {
            viewLifecycleOwner.repeatOnLifecycle(Lifecycle.State.STARTED) { selection.index.collect { wheel.render(it) } }
        }
    }

    private fun setUpWheel(wheel: NumberPicker) {
        val labels = args.getStringArray(ARG_LABELS).orEmpty()
        wheel.minValue = 0
        wheel.maxValue = (labels.size - 1).coerceAtLeast(0)
        wheel.displayedValues = labels.takeIf { it.isNotEmpty() }
        wheel.wrapSelectorWheel = false
        wheel.setOnValueChangedListener { _, _, index -> selection.select(index) }
    }

    private fun NumberPicker.render(index: Int) {
        if (value != index) value = index
    }

    private fun confirm() {
        val result = Bundle().apply { putInt(RESULT_INDEX, selection.index.value) }
        parentFragmentManager.setFragmentResult(args.getString(ARG_REQUEST).orEmpty(), result)
        dismiss()
    }

    companion object {
        private const val ARG_REQUEST = "ds_wheel_request"
        private const val ARG_TITLE = "ds_wheel_title"
        private const val ARG_LABELS = "ds_wheel_labels"
        private const val RESULT_INDEX = "ds_wheel_result"

        fun show(manager: FragmentManager, requestKey: String, title: CharSequence, labels: List<String>, selected: Int) {
            if (manager.findFragmentByTag(requestKey) != null) return
            WheelSheet().apply {
                arguments = Bundle().apply {
                    putString(ARG_REQUEST, requestKey)
                    putCharSequence(ARG_TITLE, title)
                    putStringArray(ARG_LABELS, labels.toTypedArray())
                    putInt(WheelSelection.ARG_INITIAL, selected.coerceIn(0, (labels.size - 1).coerceAtLeast(0)))
                }
            }.show(manager, requestKey)
        }

        fun chosenIndex(result: Bundle): Int? = result.getInt(RESULT_INDEX, -1).takeIf { it >= 0 }
    }
}
